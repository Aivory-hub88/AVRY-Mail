/// Aivory Email Assistant — zeroclaw vanilla sub-agent
/// Canonical system prompt + helpers. Keep in core so both api and smtp can reuse.

pub const SYSTEM_PROMPT: &str = r#"You are Aivory Email Assistant — sub-agent vanilla zeroclaw untuk Aivory Mail.
Tugas: bantu user mengelola inbox @mail.aivory.uk dengan konteks penuh.

Aturan:
- Jawab singkat, actionable, bahasa user (ID/EN).
- Jika ada email/thread context, kutip snippet relevan (max 300 char) + intent/urgency dari heuristic.
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
- Ketika user klik "Push to Mission Control" atau auto-triage High urgency, buat notification {type: "email_assistant", title, body, action_url: "https://mail.aivory.uk/?thread_id=..."}.
"#;

pub fn build_prompt(question: &str, context_summary: &str, thread_memory: Option<&str>, inbox_overview: Option<&str>) -> Vec<serde_json::Value> {
    let mut msgs = vec![serde_json::json!({"role":"system","content": SYSTEM_PROMPT})];
    if let Some(ov) = inbox_overview {
        msgs.push(serde_json::json!({"role":"system","content": format!("Inbox overview: {}", ov)}));
    }
    if let Some(mem) = thread_memory {
        msgs.push(serde_json::json!({"role":"system","content": format!("Thread memory (budgeted): {}", mem)}));
    }
    if !context_summary.is_empty() {
        msgs.push(serde_json::json!({"role":"system","content": format!("Selected message context: {}", context_summary)}));
    }
    msgs.push(serde_json::json!({"role":"user","content": question}));
    msgs
}

pub fn heuristic_fallback(question: &str, subject: &str, body: &str) -> String {
    let q = question.to_lowercase();
    if q.contains("ringkas") || q.contains("summarize") || q.contains("rangkum") {
        let snippet = if body.len() > 300 { format!("{}…", &body[..300]) } else { body.to_string() };
        return format!("Ringkasan: \"{}\" — {}\n\nNext: /summarize thread / draft reply / push ke Mission Control jika urgent.", subject, snippet);
    }
    if q.contains("draft") || q.contains("balas") || q.contains("reply") {
        return format!("Draft balasan untuk \"{}\":\n\nHi,\n\nTerima kasih atas email \"{}\". Terkait: {} — saya tindak lanjuti segera.\n\nSalam,\nAivory Email Assistant\n\n(citation: subject/body snippet)", subject, subject, &body[..body.len().min(120)]);
    }
    if q.contains("cari") || q.contains("search") || q.contains("invoice") {
        return "Gunakan search_mail(query) untuk mencari. Contoh: search_mail(\"invoice\") atau cek get_inbox_overview() untuk ringkasan. Mau saya carikan?".into();
    }
    format!("Hai, saya Aivory Email Assistant (zeroclaw vanilla). Saya bisa bantu: ringkas inbox, draft balasan, cari email, atau push ke Mission Control (dashboard.aivory.id).\n\nPertanyaan Anda: \"{}\" \n\nCoba: \"ringkas inbox hari ini\" / \"buatkan draft balasan untuk email terakhir\" / \"cari invoice overdue\"", question)
}
