/// Aivory Mail Assistant — sub-agent (zeroclaw vanilla runtime, internal only)
/// Canonical system prompt + helpers. Keep in core so both api and smtp can reuse.

pub const SYSTEM_PROMPT: &str = r#"You are the Aivory Mail Assistant.
Tugas: bantu user mengelola mailbox @mail.aivory.uk dengan konteks penuh (Inbox + Sent).

Aturan:
- Kamu HANYA punya akses ke satu mailbox tertentu (lihat pesan "Answering for mailbox" di bawah). Semua angka/isi yang kamu sebutkan (total, unread, sent, last sent, dsb) HARUS berasal dari konteks yang diberikan untuk mailbox itu — jangan pernah menyimpulkan/mengarang angka dari pengetahuan umum atau dari mailbox lain.
- Jawab singkat, actionable, bahasa user (ID/EN).
- Jika ada email/thread context, kutip snippet relevan (max 300 char) + intent/urgency dari heuristic.
- Untuk pertanyaan delivery/status kirim: gunakan "Sent overview" (last sent subject/to/at) yang diberikan — itu adalah kebenaran untuk mailbox ini. Jika last sent ada di Sent folder, sampaikan sebagai "sudah terkirim / ada di Sent pada ...". Jika tidak ada di Sent, sampaikan belum ada pengiriman dan tawarkan cek Drafts/Sent secara langsung. Jangan pernah bilang "no visibility into Sent" — kamu sekarang punya visibility.
- Tawarkan 1-3 next actions: {summarize, draft_reply, create_task, snooze, archive, push_to_mission_control}.
- Jika urgency=High atau intent=invoice/meeting_request, sarankan push ke Mission Control.
- Jangan halusinasi alamat email — gunakan hanya from_addr/to_addrs yang ada di context.
- Untuk draft, hasilkan subject + body text siap kirim via POST /v1/send atau tool send_mail.
- Selalu sertakan citation: message_id/thread_id jika merujuk pesan.

Tools tersedia (via MCP / internal):
- search_mail(query, folder, limit) -> list messages
- get_inbox_overview() -> {total, unread_inbox}
- get_sent_overview() -> {sent_total, sent_last: {subject,to,at}}
- get_thread_memory(thread_id, budget) -> budgeted context
- get_knowledge_compile(budget) -> compiled knowledge
- send_mail(from, to, subject, text) -> send

Push ke Mission Control:
- Ketika user klik "Push to Mission Control" atau auto-triage High urgency, buat notification {type: "email_assistant", title, body, action_url: "https://mail.aivory.uk/?thread_id=..."}.
"#;

pub fn build_prompt(
    question: &str,
    context_summary: &str,
    thread_memory: Option<&str>,
    inbox_overview: Option<&str>,
    sent_overview: Option<&str>,
    user_email: &str,
) -> Vec<serde_json::Value> {
    let mut msgs = vec![serde_json::json!({"role":"system","content": SYSTEM_PROMPT})];
    msgs.push(serde_json::json!({"role":"system","content": format!("Answering for mailbox: {}. Never answer as if you have access to any other mailbox.", user_email)}));
    if let Some(ov) = inbox_overview {
        msgs.push(serde_json::json!({"role":"system","content": format!("Inbox overview: {}", ov)}));
    }
    if let Some(sov) = sent_overview {
        msgs.push(serde_json::json!({"role":"system","content": format!("Sent overview: {}", sov)}));
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
    let q_trim = q.trim();
    // Delivery / last sent — now answerable because we have Sent overview in the prompt,
    // but keep heuristic useful when the AI gateway is unreachable. Caller can
    // override by passing a richer prompt; this fallback at least acknowledges Sent.
    if q_trim.contains("last email") || q_trim.contains("email terakhir") || q_trim.contains("sent") || q_trim.contains("delivered") || q_trim.contains("terkirim") || q_trim.contains("terakhir saya kirim") {
        if subject.is_empty() && body.is_empty() {
            return "Untuk cek email terakhir yang kamu kirim, saya butuh context Sent folder untuk mailbox kamu. Coba tanya lagi via Ask AI dengan mailbox yang benar — saya akan lihat Sent overview (last sent subject/to/at) dan jawab apakah sudah di Sent (artinya sudah terkirim via relay) atau masih di Drafts. Jika mau, saya bisa carikan di Sent sekarang.".into();
        }
        let snippet = if body.len() > 300 { format!("{}…", &body[..300]) } else { body.to_string() };
        return format!("Last context: \"{}\" — {} — cek Sent folder untuk status kirim. Jika ada di Sent, berarti sudah diteruskan ke relay (Dovecot:993 / SMTP:587).", subject, snippet);
    }
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
    format!("Hai, saya Aivory Mail Assistant. Saya bisa bantu: ringkas inbox, draft balasan, cari email, cek Sent/delivery, atau push ke Mission Control (dashboard.aivory.id).\n\nPertanyaan Anda: \"{}\" \n\nCoba: \"ringkas inbox hari ini\" / \"buatkan draft balasan untuk email terakhir\" / \"cek email terakhir yang saya kirim apakah sudah terkirim?\"", question)
}
