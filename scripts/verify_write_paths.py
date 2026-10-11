"""End-to-end check of the Postgres write paths that depend on column types:
contact capture from inbound mail, Block sender, mail filters (create, update,
delete, and that they fire), the vacation responder (save, auto-reply, dedupe)
and message labels.

Needs a running API and the Postgres database it uses, with a default tenant:

    INTERNAL_TOKEN=... MAIL_API_BASE=http://localhost:8095 \
        python3 scripts/verify_write_paths.py <database-name>

It provisions its own domain and mailbox (alice@acme.test) through psql, so run
it on a scratch copy, never on production data. Exits non-zero on any failure.
"""
import hashlib, json, os, subprocess, sys, time, urllib.request, uuid

BASE = os.environ.get("MAIL_API_BASE", "http://localhost:8095")
DB = sys.argv[1]
INTERNAL = os.environ["INTERNAL_TOKEN"]


def psql(sql):
    p = subprocess.run(["psql", "-h", "/tmp", "-d", DB, "-X", "-Atc", sql], capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit("psql failed: " + p.stderr)
    return p.stdout.strip()


def pw_hash(p):
    salt = os.urandom(16)
    st = hashlib.sha256(salt + p.encode()).digest()
    for _ in range(1, 1000):
        st = hashlib.sha256(st).digest()
    return "sha256$1000$%s$%s" % (salt.hex(), st.hex())


def call(method, path, body=None, token=None, headers=None):
    h = {"content-type": "application/json", **(headers or {})}
    if token:
        h["authorization"] = "Bearer " + token
    req = urllib.request.Request(BASE + path, data=json.dumps(body).encode() if body is not None else None, headers=h, method=method)
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw)
        except Exception:
            return e.code, raw.decode()[:200]


results = []


def check(name, cond, detail=""):
    results.append((name, bool(cond)))
    print(("PASS " if cond else "FAIL ") + name + ("" if cond else f"   -> {str(detail)[:200]}"))


# ── setup (SQL, like a provisioned install) ──
MB, DOM = str(uuid.uuid4()), str(uuid.uuid4())
psql(f"INSERT INTO domains (id, tenant_id, domain, status, dkim_private_key) VALUES ('{DOM}','00000000-0000-0000-0000-000000000000','acme.test','Active','k')")
psql(f"INSERT INTO mailboxes (id, tenant_id, domain_id, address, password_hash) VALUES ('{MB}','00000000-0000-0000-0000-000000000000','{DOM}','alice@acme.test','{pw_hash('pw-alice')}')")
s, j = call("POST", "/v1/auth/login", {"email": "alice@acme.test", "password": "pw-alice"})
tok = j["data"]["token"]
check("login", s == 200)


def deliver(frm, subject):
    raw = f"From: {frm}\r\nTo: alice@acme.test\r\nSubject: {subject}\r\nMessage-ID: <{uuid.uuid4()}@x>\r\n\r\nbody\r\n"
    s, j = call("POST", "/v1/webhooks/inbound", {"from": frm, "to": "alice@acme.test", "raw_text": raw}, headers={"x-internal-token": INTERNAL})
    return s, j


def folder_of(subject):
    return psql(f"SELECT folder FROM messages WHERE subject='{subject}' AND mailbox_id='{MB}' ORDER BY created_at DESC LIMIT 1")


# ── 1. contacts are captured from inbound mail ──
s, _ = deliver("budi@client.co.id", "first")
check("inbound mail accepted", s == 200)
row = psql("SELECT count(*)||'|'||bool_and(blocked::text IN ('false','f','0'))::text FROM contacts WHERE email='budi@client.co.id'")
check("contact captured from the mail (once, not blocked)", row == "1|true", row)
deliver("budi@client.co.id", "second")
check("second mail from the same sender updates, no duplicate", psql("SELECT count(*) FROM contacts WHERE email='budi@client.co.id'") == "1")
check("first mail went to the Inbox", folder_of("first") == "Inbox", folder_of("first"))

# ── 2. Block sender ──
s, j = call("POST", "/v1/contacts/block", {"email": "budi@client.co.id", "mailbox_id": MB}, tok)
check("block sender -> 200", s == 200, (s, j))
check("contact is now blocked", psql("SELECT blocked::text FROM contacts WHERE email='budi@client.co.id'") in ("true", "t", "1"))
check("a 'Block ...' filter was created, enabled", psql("SELECT count(*) FROM mail_filters WHERE name='Block budi@client.co.id' AND enabled::text IN ('true','t','1')") == "1")
deliver("budi@client.co.id", "after-block")
check("mail from a blocked sender no longer lands in the Inbox", folder_of("after-block") in ("Spam", "Trash"), folder_of("after-block"))

# ── 3. filters: create / list / update / delete ──
s, j = call("POST", "/v1/filters", {"name": "promo to spam", "mailbox_id": MB, "criteria": {"from": "promo@shop.example"}, "action": {"move": "Spam"}, "priority": 5, "enabled": True}, tok)
check("create filter -> 201", s == 201, (s, j))
fid = j.get("data", {}).get("id") if isinstance(j, dict) else None
s, j = call("GET", f"/v1/filters?mailbox_id={MB}", token=tok)
check("list shows it", s == 200 and any(f.get("id") == fid for f in j.get("data", [])), (s, j))
s, j = call("PUT", f"/v1/filters/{fid}", {"enabled": False, "name": "renamed", "priority": 2}, tok)
st = psql(f"SELECT name||'|'||enabled::text||'|'||priority FROM mail_filters WHERE id::text='{fid}'")
check("update filter (name, enabled, priority)", s == 200 and st in ("renamed|false|2", "renamed|f|2", "renamed|0|2"), (s, j, st))
deliver("promo@shop.example", "promo-disabled")
check("a disabled filter does not fire", folder_of("promo-disabled") == "Inbox", folder_of("promo-disabled"))
call("PUT", f"/v1/filters/{fid}", {"enabled": True}, tok)
deliver("promo@shop.example", "promo-enabled")
check("re-enabled filter fires", folder_of("promo-enabled") == "Spam", folder_of("promo-enabled"))
s, j = call("DELETE", f"/v1/filters/{fid}", token=tok)
check("delete filter", s == 200 and psql(f"SELECT count(*) FROM mail_filters WHERE id::text='{fid}'") == "0", (s, j))
s, j = call("PUT", "/v1/filters/not-a-uuid", {"enabled": True}, tok)
check("a malformed filter id is a clean 404, not a 500", s in (400, 404), (s, j))

# ── 4. vacation responder ──
s, j = call("GET", f"/v1/vacation?mailbox_id={MB}", token=tok)
check("vacation: nothing saved yet", s == 200 and j["data"].get("enabled") is False, (s, j))
s, j = call("POST", "/v1/vacation", {"mailbox_id": MB, "enabled": True, "subject": "Away", "body": "Back Monday"}, tok)
check("vacation: save -> 200", s == 200, (s, j))
s, j = call("POST", "/v1/vacation", {"mailbox_id": MB, "enabled": True, "subject": "Away 2", "body": "Back Tuesday"}, tok)
check("vacation: save again updates, still one row", s == 200 and psql(f"SELECT count(*) FROM vacation_responders WHERE mailbox_id='{MB}'") == "1", (s, j))
s, j = call("GET", f"/v1/vacation?mailbox_id={MB}", token=tok)
check("vacation: read back the latest text", s == 200 and j["data"].get("body") == "Back Tuesday" and j["data"].get("enabled") is True, (s, j))
deliver("carol@other.example", "hello")
time.sleep(1.5)
n1 = psql("SELECT count(*) FROM vacation_deliveries WHERE recipient='carol@other.example'")
t1 = psql("SELECT sent_at FROM vacation_deliveries WHERE recipient='carol@other.example'")
check("vacation: auto-reply recorded for the sender", n1 == "1", n1)
deliver("carol@other.example", "hello again")
time.sleep(1.5)
t2 = psql("SELECT sent_at FROM vacation_deliveries WHERE recipient='carol@other.example'")
check("vacation: a second mail inside the interval is not answered again", t1 == t2 and psql("SELECT count(*) FROM vacation_deliveries WHERE recipient='carol@other.example'") == "1", (t1, t2))
s, j = call("POST", "/v1/vacation", {"mailbox_id": MB, "enabled": False}, tok)
check("vacation: switch off", s == 200)

# ── 5. labels on a message ──
s, j = call("POST", "/v1/labels", {"name": "Work", "mailbox_id": MB}, tok)
lid = j.get("data", {}).get("id") if isinstance(j, dict) else None
check("create label", s in (200, 201) and lid, (s, j))
mid = psql(f"SELECT id FROM messages WHERE subject='first' AND mailbox_id='{MB}' LIMIT 1")
s, j = call("POST", f"/v1/messages/{mid}/labels", {"label_id": lid}, tok)
check("attach label to a message", s == 200 and psql(f"SELECT count(*) FROM message_labels WHERE message_id::text='{mid}'") == "1", (s, j))
s, j = call("POST", f"/v1/messages/{mid}/labels", {"label_id": lid}, tok)
check("attaching twice is harmless", s == 200 and psql(f"SELECT count(*) FROM message_labels WHERE message_id::text='{mid}'") == "1", (s, j))
s, j = call("GET", f"/v1/messages/{mid}/labels", token=tok)
check("list shows the label", s == 200 and [l["name"] for l in j.get("data", [])] == ["Work"], (s, j))
s, j = call("DELETE", f"/v1/messages/{mid}/labels/{lid}", token=tok)
check("detach", s == 200 and psql(f"SELECT count(*) FROM message_labels WHERE message_id::text='{mid}'") == "0", (s, j))

fails = [n for n, ok in results if not ok]
print(json.dumps({"passed": len(results) - len(fails), "total": len(results), "failed": fails}))
sys.exit(1 if fails else 0)
