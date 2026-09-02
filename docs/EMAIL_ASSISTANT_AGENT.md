# Aivory Email Assistant — Sub-Agent (Zeroclaw Vanilla)

> Sub-agent khusus untuk `Aivory Mail` yang hidup di atas **zeroclaw vanilla** (OpenRouter gateway) + `MAIL_MODE=hybrid`. Bertanggung jawab sebagai *copilot* inbox: triage, summarize, draft, search, dan **push ke Mission Control** (`dashboard.aivory.id`).

## Identity

| Field | Value |
|-------|-------|
| **Agent ID** | `agt-email-assistant-001` |
| **Name** | `Aivory Email Assistant` |
| **Runtime** | `zeroclaw` (vanilla) — `aivory_mail_core::email_assistant` + `aivory_mail_api::api::ai_chat` |
| **Model** | `MAIL_INTELLIGENCE_MODEL=deepseek/deepseek-v4-flash-0731` fallback `qwen/qwen3-235b-a22b` |
| **Gateway** | `AI_GATEWAY_URL` (`http://avry-zeroclaw-daemon:3010`) → fallback `OPENROUTER_API_KEY` direct |
| **Tools (MCP)** | `search_mail`, `get_inbox_overview`, `get_thread_memory`, `get_knowledge_compile`, `send_mail` (`/mcp`) |
| **Storage** | `ai_chat_history` + `mission_control_notifications` (migration `012_email_assistant.sql`) |
| **Push target** | `GET /v1/notifications` (polled by Mission Control widget) + `WORKFLOW_URL/webhook/email-assistant` + RealtimeHub WS |

## System Prompt (canonical)

```
You are Aivory Email Assistant — sub-agent vanilla zeroclaw untuk Aivory Mail.
Tugas: bantu user mengelola inbox @mail.aivory.uk dengan konteks penuh.

Aturan:
- Jawab singkat, actionable, bahasa user (ID/EN).
- Jika ada email/thread context, kutip snippet relevan (max 300 char) + intent/urgency dari heuristic aivory_mail_core::intelligence.
- Tawarkan 1-3 next actions: {summarize, draft_reply, create_task, snooze, archive, push_to_mission_control}.
- Jika urgency=High atau intent=invoice/meeting_request, sarankan push ke Mission Control.
- Jangan halusinasi alamat email — gunakan hanya from_addr/to_addrs yang ada di context.
- Untuk draft, hasilkan subject + body text siap kirim via POST /v1/send atau tool send_mail.
- Selalu sertakan citation: message_id/thread_id jika merujuk pesan.

Tools tersedia (via MCP / internal):
- search_mail(query, folder, limit) -> list messages
- get_inbox_overview() -> {total, unread_inbox}
- get_thread_memory(thread_id, budget) -> budgeted context
- get_knowledge_compile(budget) -> compiled knowledge
- send_mail(from, to, subject, text) -> send

Push ke Mission Control:
- Ketika user klik "Push to Mission Control" atau auto-triage High urgency, buat notification {type: "email_assistant", title, body, action_url: "https://mail.aivory.uk/?thread_id=..."} dan simpan di mission_control_notifications. Dashboard akan poll GET /v1/notifications.
```

## Flow

```
[User] -- Ask AI (web AskAIAssistant.tsx) -- POST /v1/ai/ask {question, context, history}
    |
    v
[Avry-Mail API ai_chat.rs]
    ├─ 1. Ambil mailbox/thread/message context dari DB (search.rs / threads)
    ├─ 2. Heuristic analyze (classify_intent, detect_urgency, extract_entities)
    ├─ 3. Build prompt = system_prompt + thread_memory(budget=2000) + inbox_overview + question
    ├─ 4. Try AI_GATEWAY_URL/v1/ai/chat (zeroclaw vanilla, x-internal-token) 8s timeout
    ├─ 5. Fallback OpenRouter https://openrouter.ai/api/v1/chat/completions (model MAIL_INTELLIGENCE_MODEL) 10s
    ├─ 6. Fallback heuristic answer (summarize + suggested_actions)
    ├─ 7. Simpan ai_chat_history
    ├─ 8. Jika urgency High / user minta push → POST internal push_to_mission_control (insert + RealtimeHub + WORKFLOW_URL webhook)
    └─ 9. Return {answer, sources, suggested_actions, pushed}
```

## Zeroclaw Vanilla Wiring

- **Daemon**: `AI_GATEWAY_URL=http://avry-zeroclaw-daemon:3010` (dari `~/AVRY-V2-Main/.env` / `services/avry-mail/.env`).
- **Auth**: `x-internal-token: $INTERNAL_TOKEN` (sama dengan `mcp.rs:35`).
- **Endpoint kontrak**: `POST /v1/ai/chat` body `{model, messages:[{role,content}], temperature:0.3}` → `{choices:[{message:{content}}]}` (OpenAI-compatible). Jika daemon tidak ada, langsung ke OpenRouter.
- **Sub-agent registration** (opsional, untuk dashboard Agents page):
  ```bash
  curl -X POST https://backend.aivory.id/api/v1/agents \
    -H "Authorization: Bearer $JWT" \
    -d '{"agent_id":"agt-email-assistant-001","name":"Aivory Email Assistant","type":"email_assistant","runtime":"zeroclaw","status":"active"}'
  ```

## Mission Control Integration

- **Poll**: `GET https://mail.aivory.uk/v1/notifications?limit=20` (CORS `https://dashboard.aivory.id` di `config.rs:73`). Widget `frontend/avry-user-dashboard/components/office/EmailAssistantWidget.tsx` fetch tiap 15s.
- **Push**: `POST /v1/ai/push-to-mission-control` body `{title, body, type, action_url, metadata}` → insert `mission_control_notifications` + broadcast `RealtimeHub` WS `/v1/realtime/ws?mailbox_id=` + optional `POST $WORKFLOW_URL/webhook/email-assistant-notify` untuk n8n → Slack/Telegram.
- **Realtime**: Mission Control bisa juga subscribe WS `wss://mail.aivory.uk/v1/realtime/ws` jika ingin live.

## Files

- `crates/aivory-mail-core/src/email_assistant.rs` — system prompt + prompt builder
- `crates/aivory-mail-api/src/api/ai_chat.rs` — handlers `ask`, `history`, `push`, `list_notifications`
- `migrations/012_email_assistant.sql` — Postgres; `main.rs::ensure_schema` menambah SQLite fallback
- `web/components/AskAIAssistant.tsx` — panel chat di `web/app/page.tsx:740` (detail pane)
- `frontend/avry-user-dashboard/components/office/EmailAssistantWidget.tsx` — widget Mission Control

## Test

```bash
# login
TOKEN=$(curl -s -X POST https://mail.aivory.uk/v1/auth/login -H 'Content-Type: application/json' -d '{"email":"admin@aivory.id","password":"Avry786876!@"}' | jq -r .data.token)

# ask
curl -s -X POST https://mail.aivory.uk/v1/ai/ask -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{"question":"ringkas inbox hari ini","context":{}}' | jq

# push
curl -s -X POST https://mail.aivory.uk/v1/ai/push-to-mission-control -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{"title":"Invoice overdue","body":"INV-123 perlu follow up","action_url":"https://mail.aivory.uk"}' | jq

# mission control poll (public, tapi butuh auth untuk detail; list notifications allow anon read untuk widget)
curl -s https://mail.aivory.uk/v1/notifications | jq

# WS
wscat -c "wss://mail.aivory.uk/v1/realtime/ws?mailbox_id=default"
```

## Roadmap

- STARTTLS SMTP ingress (sudah ada `avry-mail-smtp:2587`)
- Agent auto-triage cron (setiap 5 menit scan Inbox unread High → auto push)
- Memory: simpan thread summary di `knowledge_cache` via Cognee
