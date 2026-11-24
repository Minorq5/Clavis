// Сервер Clavis: активация по одноразовым кодам, учёт устройств и панель владельца.
// Сервер не хранит ни одного кода в открытом виде, только SHA-256 от него.
// Каждый запрос устройства подписан его ключом Ed25519; запросы владельца
// принимаются только с подписью ключа, который активировал код владельца.

import { createClient } from "npm:@supabase/supabase-js@2";
import nacl from "npm:tweetnacl@1.0.3";

const db = createClient(Deno.env.get("SUPABASE_URL")!, Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!, {
  auth: { persistSession: false },
});

const MAX_FAILURES = 15;
const IP_LIMIT_PER_HOUR = 3000;
const CLOCK_SKEW_MS = 5 * 60_000;
const ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // без 0, O, 1, I

const enc = new TextEncoder();
const b64 = {
  to: (u: Uint8Array) => btoa(String.fromCharCode(...u)),
  from: (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0)),
};

async function sha256(s: string): Promise<string> {
  const h = new Uint8Array(await crypto.subtle.digest("SHA-256", enc.encode(s)));
  return [...h].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function normalizeCode(code: string): string {
  return String(code ?? "").toUpperCase().replace(/[^A-Z0-9]/g, "");
}

function newCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(24));
  const raw = [...bytes].map((b) => ALPHABET[b & 31]).join("");
  return raw.replace(/(.{4})(?=.)/g, "$1-");
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
}

let serverKeys: { sk: Uint8Array; pk: Uint8Array } | null = null;
async function keys() {
  if (serverKeys) return serverKeys;
  const { data } = await db.from("server_keys").select("sk, pk").eq("id", 1).maybeSingle();
  if (data) {
    serverKeys = { sk: b64.from(data.sk), pk: b64.from(data.pk) };
    return serverKeys;
  }
  const kp = nacl.sign.keyPair();
  await db.from("server_keys").insert({ id: 1, sk: b64.to(kp.secretKey), pk: b64.to(kp.publicKey) });
  const { data: again } = await db.from("server_keys").select("sk, pk").eq("id", 1).single();
  serverKeys = { sk: b64.from(again!.sk), pk: b64.from(again!.pk) };
  return serverKeys;
}

function verify(pubkey: string, msg: string, sig: string): boolean {
  try {
    const pk = b64.from(pubkey);
    const s = b64.from(sig);
    return pk.length === 32 && s.length === 64 && nacl.sign.detached.verify(enc.encode(msg), s, pk);
  } catch {
    return false;
  }
}

async function sign(msg: string): Promise<string> {
  const { sk } = await keys();
  return b64.to(nacl.sign.detached(enc.encode(msg), sk));
}

/** Проверяет подпись конверта и защищает от повторной отправки. */
async function openEnvelope(body: any): Promise<{ ok: true; payload: any } | { ok: false; error: string }> {
  const { op, pubkey, ts, nonce, payload, sig } = body ?? {};
  if (typeof op !== "string" || typeof pubkey !== "string" || typeof ts !== "number" || typeof nonce !== "string" || typeof payload !== "string" || typeof sig !== "string") {
    return { ok: false, error: "malformed" };
  }
  if (Math.abs(Date.now() - ts) > CLOCK_SKEW_MS) return { ok: false, error: "clock" };
  if (nonce.length < 16 || nonce.length > 64) return { ok: false, error: "malformed" };
  if (!verify(pubkey, `${op}\n${ts}\n${nonce}\n${payload}`, sig)) return { ok: false, error: "signature" };
  const { error } = await db.from("used_nonces").insert({ nonce });
  if (error) return { ok: false, error: "replay" };
  try {
    return { ok: true, payload: JSON.parse(payload) };
  } catch {
    return { ok: false, error: "malformed" };
  }
}

async function attemptsLeft(hw: string) {
  const { data } = await db.from("hw_attempts").select("failures, blocked").eq("hw_hash", hw).maybeSingle();
  if (!data) return { attemptsLeft: MAX_FAILURES, blocked: false };
  return { attemptsLeft: Math.max(0, MAX_FAILURES - data.failures), blocked: data.blocked };
}

async function isOwner(pubkey: string) {
  const { data } = await db.from("owner").select("pubkey").eq("id", 1).maybeSingle();
  return !!data && data.pubkey === pubkey;
}

const validHw = (hw: unknown): hw is string => typeof hw === "string" && /^[0-9a-f]{64}$/.test(hw);
const clean = (s: unknown, max: number) => String(s ?? "").replace(/[\u0000-\u001f\u007f]/g, " ").trim().slice(0, max);


async function deviceOf(pubkey: string) {
  const { data } = await db.from("devices").select("id, revoked").eq("pubkey", pubkey).maybeSingle();
  return data && !data.revoked ? (data.id as string) : null;
}

async function roleIn(doc: string, device: string): Promise<"owner" | "editor" | null> {
  const { data } = await db.from("doc_members").select("role").eq("doc_id", doc).eq("device_id", device).maybeSingle();
  return (data?.role as "owner" | "editor") ?? null;
}

async function currentEpoch(doc: string): Promise<number | null> {
  const { data } = await db.from("shared_docs").select("epoch").eq("id", doc).maybeSingle();
  return data ? data.epoch : null;
}

const isUuid = (s: unknown): s is string => typeof s === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(s);
const isB64 = (s: unknown, max: number): s is string => typeof s === "string" && s.length > 0 && s.length <= max && /^[A-Za-z0-9+/]+=*$/.test(s);
const isHex64 = (s: unknown): s is string => typeof s === "string" && /^[0-9a-f]{64}$/.test(s);

const MB = 1024 * 1024;

/** Операции с общими документами. Возвращает null, если операция не относится к ним. */
async function sharedOp(op: string, p: any, me: string): Promise<Response | null> {
  switch (op) {
    case "register_kx": {
      if (!isB64(p.kx_pub, 64)) return json({ error: "malformed" }, 400);
      await db.from("devices").update({ kx_pub: p.kx_pub, display_name: clean(p.name, 40) }).eq("id", me);
      return json({ ok: true, device: me });
    }

    case "doc_create": {
      if (!isUuid(p.doc_id) || !isB64(p.wrapped, 400) || (p.meta_ct != null && !isB64(p.meta_ct, 4000))) return json({ error: "malformed" }, 400);
      const { error } = await db.from("shared_docs").insert({ id: p.doc_id, owner_device: me, epoch: 1, meta_ct: p.meta_ct ?? null });
      if (error) return json({ error: "exists" }, 409);
      await db.from("doc_members").insert({ doc_id: p.doc_id, device_id: me, role: "owner" });
      await db.from("doc_keys").insert({ doc_id: p.doc_id, device_id: me, epoch: 1, wrapped: p.wrapped });
      return json({ ok: true });
    }

    case "doc_list": {
      const { data: mem } = await db.from("doc_members").select("doc_id, role").eq("device_id", me).limit(500);
      const ids = (mem ?? []).map((m) => m.doc_id);
      if (!ids.length) return json([]);
      const [{ data: docs }, { data: keys }] = await Promise.all([
        db.from("shared_docs").select("id, epoch, meta_ct, updated_at").in("id", ids),
        db.from("doc_keys").select("doc_id, epoch, wrapped").eq("device_id", me).in("doc_id", ids),
      ]);
      return json((docs ?? []).map((d) => ({
        id: d.id,
        role: mem!.find((m) => m.doc_id === d.id)?.role,
        epoch: d.epoch,
        metaCt: d.meta_ct,
        updated: Date.parse(d.updated_at),
        keys: (keys ?? []).filter((k) => k.doc_id === d.id).map((k) => ({ epoch: k.epoch, wrapped: k.wrapped })),
      })));
    }

    case "doc_members": {
      if (!isUuid(p.doc_id) || !(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      const { data } = await db.from("doc_members").select("device_id, role, devices(display_name, name, kx_pub)").eq("doc_id", p.doc_id);
      return json((data ?? []).map((m: any) => ({
        device: m.device_id,
        role: m.role,
        name: m.devices?.display_name || m.devices?.name || "",
        kxPub: m.devices?.kx_pub ?? null,
        me: m.device_id === me,
      })));
    }

    case "doc_pull": {
      if (!isUuid(p.doc_id)) return json({ error: "malformed" }, 400);
      if (!(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      const after = Number.isSafeInteger(p.after) ? p.after : 0;
      const [{ data: doc }, { data: snap }, { data: keys }] = await Promise.all([
        db.from("shared_docs").select("epoch, meta_ct").eq("id", p.doc_id).single(),
        db.from("doc_snapshots").select("epoch, upto_id, ct").eq("doc_id", p.doc_id).maybeSingle(),
        db.from("doc_keys").select("epoch, wrapped").eq("doc_id", p.doc_id).eq("device_id", me),
      ]);
      const useSnap = snap && after < snap.upto_id;
      const from = useSnap ? snap.upto_id : after;
      const { data: ups } = await db.from("doc_updates").select("id, epoch, ct").eq("doc_id", p.doc_id).gt("id", from).order("id").limit(1000);
      const { count } = await db.from("doc_updates").select("id", { count: "exact", head: true }).eq("doc_id", p.doc_id);
      return json({
        epoch: doc!.epoch,
        metaCt: doc!.meta_ct,
        keys: keys ?? [],
        snapshot: useSnap ? { epoch: snap.epoch, upto: snap.upto_id, ct: snap.ct } : null,
        updates: ups ?? [],
        more: (ups ?? []).length === 1000,
        total: count ?? 0,
      });
    }

    case "doc_append": {
      if (!isUuid(p.doc_id) || !Array.isArray(p.cts) || p.cts.length === 0 || p.cts.length > 100) return json({ error: "malformed" }, 400);
      if (!p.cts.every((c: unknown) => isB64(c, 12 * MB))) return json({ error: "malformed" }, 400);
      if (!(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      const epoch = await currentEpoch(p.doc_id);
      if (epoch !== p.epoch) return json({ error: "epoch", epoch }, 409);
      const { data, error } = await db.from("doc_updates").insert(p.cts.map((ct: string) => ({ doc_id: p.doc_id, epoch, sender: me, ct }))).select("id");
      if (error) return json({ error: "server" }, 500);
      await db.from("shared_docs").update({ updated_at: new Date().toISOString() }).eq("id", p.doc_id);
      return json({ ids: (data ?? []).map((r) => r.id) });
    }

    case "doc_snapshot": {
      if (!isUuid(p.doc_id) || !isB64(p.ct, 20 * MB) || !Number.isSafeInteger(p.upto)) return json({ error: "malformed" }, 400);
      if (!(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      const epoch = await currentEpoch(p.doc_id);
      if (epoch !== p.epoch) return json({ error: "epoch", epoch }, 409);
      const { data: old } = await db.from("doc_snapshots").select("upto_id").eq("doc_id", p.doc_id).maybeSingle();
      if (old && old.upto_id >= p.upto) return json({ ok: true });
      await db.from("doc_snapshots").upsert({ doc_id: p.doc_id, epoch, upto_id: p.upto, ct: p.ct, created_at: new Date().toISOString() });
      await db.from("doc_updates").delete().eq("doc_id", p.doc_id).lte("id", p.upto);
      return json({ ok: true });
    }

    case "doc_meta": {
      if (!isUuid(p.doc_id) || !isB64(p.meta_ct, 4000)) return json({ error: "malformed" }, 400);
      if (!(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      const epoch = await currentEpoch(p.doc_id);
      if (epoch !== p.epoch) return json({ error: "epoch", epoch }, 409);
      await db.from("shared_docs").update({ meta_ct: p.meta_ct, updated_at: new Date().toISOString() }).eq("id", p.doc_id);
      return json({ ok: true });
    }

    case "key_put": {
      if (!isUuid(p.doc_id) || !isB64(p.wrapped, 400) || !Number.isSafeInteger(p.epoch)) return json({ error: "malformed" }, 400);
      if (!(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      await db.from("doc_keys").upsert({ doc_id: p.doc_id, device_id: me, epoch: p.epoch, wrapped: p.wrapped });
      return json({ ok: true });
    }

    case "invite_create": {
      if (!isUuid(p.doc_id) || !isHex64(p.token_hash) || !isB64(p.wrapped, 400)) return json({ error: "malformed" }, 400);
      if ((await roleIn(p.doc_id, me)) !== "owner") return json({ error: "forbidden" }, 403);
      const epoch = await currentEpoch(p.doc_id);
      if (epoch !== p.epoch) return json({ error: "epoch", epoch }, 409);
      const hours = Math.min(168, Math.max(1, Number(p.hours) || 72));
      const { count } = await db.from("doc_invites").select("token_hash", { count: "exact", head: true }).eq("doc_id", p.doc_id).is("used_at", null);
      if ((count ?? 0) >= 50) return json({ error: "limit" }, 429);
      await db.from("doc_invites").insert({
        token_hash: p.token_hash, doc_id: p.doc_id, epoch, wrapped: p.wrapped, created_by: me,
        expires_at: new Date(Date.now() + hours * 3_600_000).toISOString(),
      });
      return json({ ok: true });
    }

    case "invite_revoke_all": {
      if (!isUuid(p.doc_id)) return json({ error: "malformed" }, 400);
      if ((await roleIn(p.doc_id, me)) !== "owner") return json({ error: "forbidden" }, 403);
      await db.from("doc_invites").delete().eq("doc_id", p.doc_id).is("used_at", null);
      return json({ ok: true });
    }

    case "invite_accept": {
      if (!isUuid(p.doc_id) || !isHex64(p.token_hash) || !isB64(p.kx_pub, 64)) return json({ error: "malformed" }, 400);
      await db.from("devices").update({ kx_pub: p.kx_pub, display_name: clean(p.name, 40) }).eq("id", me);
      const existing = await roleIn(p.doc_id, me);
      const { data } = await db.rpc("accept_invite", { p_hash: p.token_hash, p_doc: p.doc_id, p_device: me });
      const row = data?.[0];
      if (!row) return json({ error: existing ? "already_member" : "invite" }, 404);
      return json({ epoch: row.epoch, wrapped: row.wrapped });
    }

    case "member_remove": {
      if (!isUuid(p.doc_id) || !isUuid(p.device) || !Array.isArray(p.keys) || !isB64(p.snapshot, 20 * MB) || !Number.isSafeInteger(p.upto)) {
        return json({ error: "malformed" }, 400);
      }
      if ((await roleIn(p.doc_id, me)) !== "owner" || p.device === me) return json({ error: "forbidden" }, 403);
      const epoch = await currentEpoch(p.doc_id);
      if (epoch === null || p.epoch !== epoch + 1) return json({ error: "epoch", epoch }, 409);
      await db.from("doc_members").delete().eq("doc_id", p.doc_id).eq("device_id", p.device);
      await db.from("doc_keys").delete().eq("doc_id", p.doc_id).eq("device_id", p.device);
      const { data: rest } = await db.from("doc_members").select("device_id").eq("doc_id", p.doc_id);
      const remaining = new Set((rest ?? []).map((r) => r.device_id));
      const keys = p.keys.filter((k: any) => remaining.has(k.device) && isB64(k.wrapped, 400));
      if (keys.length !== remaining.size) return json({ error: "keys" }, 400);
      await db.from("doc_keys").insert(keys.map((k: any) => ({ doc_id: p.doc_id, device_id: k.device, epoch: p.epoch, wrapped: k.wrapped })));
      await db.from("shared_docs").update({ epoch: p.epoch, meta_ct: isB64(p.meta_ct, 4000) ? p.meta_ct : null, updated_at: new Date().toISOString() }).eq("id", p.doc_id);
      await db.from("doc_snapshots").upsert({ doc_id: p.doc_id, epoch: p.epoch, upto_id: p.upto, ct: p.snapshot, created_at: new Date().toISOString() });
      await db.from("doc_updates").delete().eq("doc_id", p.doc_id).lte("id", p.upto);
      await db.from("doc_invites").delete().eq("doc_id", p.doc_id).is("used_at", null);
      return json({ ok: true });
    }

    case "doc_leave": {
      if (!isUuid(p.doc_id)) return json({ error: "malformed" }, 400);
      const role = await roleIn(p.doc_id, me);
      if (role === "owner") {
        await db.from("shared_docs").delete().eq("id", p.doc_id).eq("owner_device", me);
      } else if (role) {
        await db.from("doc_members").delete().eq("doc_id", p.doc_id).eq("device_id", me);
        await db.from("doc_keys").delete().eq("doc_id", p.doc_id).eq("device_id", me);
      }
      return json({ ok: true });
    }

    case "blob_put": {
      if (!isUuid(p.doc_id) || !isUuid(p.blob_id) || !isB64(p.ct, 12 * MB)) return json({ error: "malformed" }, 400);
      if (!(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      const epoch = await currentEpoch(p.doc_id);
      const { data: mem } = await db.from("doc_members").select("device_id").eq("doc_id", p.doc_id);
      const pending = (mem ?? []).map((m) => m.device_id).filter((d) => d !== me);
      if (!pending.length) return json({ ok: true, stored: false });
      const { error } = await db.from("doc_blobs").insert({ id: p.blob_id, doc_id: p.doc_id, epoch, ct: p.ct, pending });
      return json({ ok: !error, stored: !error });
    }

    case "blob_get": {
      if (!isUuid(p.doc_id) || !isUuid(p.blob_id)) return json({ error: "malformed" }, 400);
      if (!(await roleIn(p.doc_id, me))) return json({ error: "forbidden" }, 403);
      const { data } = await db.from("doc_blobs").select("epoch, ct").eq("id", p.blob_id).eq("doc_id", p.doc_id).maybeSingle();
      return data ? json(data) : json({ error: "missing" }, 404);
    }

    case "erase_me": {
      // Право на удаление: доступ закрывается сразу, данные стираются через 48 часов.
      const { data: dev } = await db.from("devices").select("id, pubkey").eq("id", me).maybeSingle();
      if (!dev || (await isOwner(dev.pubkey))) return json({ error: "forbidden" }, 403);
      const at = new Date(Date.now() + ERASE_DELAY_MS).toISOString();
      await db.from("devices").update({ revoked: true, erase_at: at }).eq("id", me);
      return json({ ok: true, eraseAt: at });
    }

    case "blob_ack": {
      if (!isUuid(p.doc_id) || !isUuid(p.blob_id)) return json({ error: "malformed" }, 400);
      await db.rpc("ack_blob", { p_blob: p.blob_id, p_device: me });
      return json({ ok: true });
    }
  }
  return null;
}

const ERASE_DELAY_MS = 48 * 3_600_000;
let lastEraseCheck = 0;

/** Стирает данные устройств, чей срок удаления наступил: запросы, попытки, само устройство и его общие заметки. */
async function eraseDue() {
  const { data } = await db.from("devices").select("id, hw_hash").lte("erase_at", new Date().toISOString()).limit(100);
  for (const d of data ?? []) {
    await db.from("requests").delete().eq("hw_hash", d.hw_hash);
    await db.from("hw_attempts").delete().eq("hw_hash", d.hw_hash).eq("blocked", false);
    await db.from("devices").delete().eq("id", d.id);
  }
}

async function handle(req: Request): Promise<Response> {
  if (req.method !== "POST") return json({ error: "method" }, 405);

  const ip = (req.headers.get("x-forwarded-for") ?? "").split(",")[0].trim() || "unknown";
  const { data: allowed } = await db.rpc("hit_ip", { p_ip: await sha256(ip), p_limit: IP_LIMIT_PER_HOUR });
  if (allowed === false) return json({ error: "rate" }, 429);

  if (Math.random() < 0.02) {
    const old = new Date(Date.now() - 2 * CLOCK_SKEW_MS).toISOString();
    await db.from("used_nonces").delete().lt("at", old);
    await db.from("ip_attempts").delete().lt("window_start", new Date(Date.now() - 2 * 3_600_000).toISOString());
    await db.from("doc_blobs").delete().lt("created_at", new Date(Date.now() - 30 * 86_400_000).toISOString());
    await db.from("doc_invites").delete().lt("expires_at", new Date(Date.now() - 86_400_000).toISOString());
    // Рассмотренные запросы доступа хранятся 30 дней, нерассмотренные 90.
    await db.from("requests").delete().neq("status", "pending").lt("created_at", new Date(Date.now() - 30 * 86_400_000).toISOString());
    await db.from("requests").delete().eq("status", "pending").lt("created_at", new Date(Date.now() - 90 * 86_400_000).toISOString());
  }
  // Отложенные удаления проверяем не реже раза в 10 минут, пока сервер получает запросы.
  if (Date.now() - lastEraseCheck > 10 * 60_000) {
    lastEraseCheck = Date.now();
    await eraseDue();
  }

  let body: any;
  try {
    body = await req.json();
  } catch {
    return json({ error: "malformed" }, 400);
  }

  if (body?.op === "server_key") {
    return json({ pk: b64.to((await keys()).pk) });
  }

  if (body?.op === "status") {
    if (!validHw(body.hw)) return json({ error: "malformed" }, 400);
    return json(await attemptsLeft(body.hw));
  }

  // Активация и запросы доступа ограничены строже, чем синхронизация.
  if (body?.op === "activate" || body?.op === "request") {
    const { data: ok } = await db.rpc("hit_ip", { p_ip: await sha256(ip + ":auth"), p_limit: 30 });
    if (ok === false) return json({ error: "rate" }, 429);
  }

  const env = await openEnvelope(body);
  if (!env.ok) return json({ error: env.error }, 400);
  const p = env.payload;
  const pubkey: string = body.pubkey;

  if (body.op !== "activate" && body.op !== "request" && body.op !== "check" && body.op !== "owner") {
    const me = await deviceOf(pubkey);
    if (!me) return json({ error: "forbidden" }, 403);
    const r = await sharedOp(body.op, p, me);
    if (r) return r;
    return json({ error: "unknown" }, 400);
  }

  switch (body.op) {
    case "activate": {
      if (!validHw(p.hw)) return json({ error: "malformed" }, 400);
      const state = await attemptsLeft(p.hw);
      if (state.blocked) return json({ ok: false, blocked: true, attemptsLeft: 0 });

      const code = normalizeCode(p.code);
      const claimed = code.length === 24 ? (await db.rpc("claim_code", { p_hash: await sha256(code) })).data?.[0] : null;
      if (!claimed) {
        const { data } = await db.rpc("register_failure", { p_hw: p.hw, p_name: clean(p.name, 80), p_limit: MAX_FAILURES });
        await db.from("hw_attempts").update({ last_ip: ip }).eq("hw_hash", p.hw);
        const row = data?.[0] ?? { failures: MAX_FAILURES, blocked: true };
        return json({ ok: false, blocked: row.blocked, attemptsLeft: Math.max(0, MAX_FAILURES - row.failures) });
      }

      const { error } = await db.from("devices").insert({
        pubkey, hw_hash: p.hw, name: clean(p.name, 80), label: claimed.label, code_id: claimed.id, last_ip: ip,
      });
      if (error) return json({ ok: false, error: "device_exists" }, 409);

      // Новый код владельца передаёт права этому компьютеру. Такие коды создаются только вручную в базе.
      let owner = false;
      if (claimed.is_owner) {
        const { error: ownErr } = await db.from("owner").upsert({ id: 1, pubkey });
        owner = !ownErr;
      }
      await db.from("hw_attempts").update({ failures: 0 }).eq("hw_hash", p.hw);

      const issuedAt = Date.now();
      const license = JSON.stringify({ pubkey, hw: p.hw, owner, iat: issuedAt });
      return json({ ok: true, owner, license, licenseSig: await sign(license) });
    }

    case "request": {
      if (!validHw(p.hw)) return json({ error: "malformed" }, 400);
      const name = clean(p.name, 40);
      if (!name) return json({ error: "malformed" }, 400);
      const { count } = await db.from("requests").select("id", { count: "exact", head: true }).eq("hw_hash", p.hw).eq("status", "pending");
      if ((count ?? 0) >= 3) return json({ ok: true });
      await db.from("requests").insert({ name, message: clean(p.message, 280), hw_hash: p.hw, ip });
      return json({ ok: true });
    }

    case "check": {
      const { data } = await db.from("devices").select("id, revoked, hw_hash").eq("pubkey", pubkey).maybeSingle();
      if (!data) return json({ ok: false, revoked: true });
      if (data.revoked) return json({ ok: false, revoked: true, owner: false });
      await db.from("devices").update({ last_seen: new Date().toISOString(), last_ip: ip }).eq("id", data.id);
      // Свежая лицензия: так устройство узнаёт, что стало владельцем или перестало им быть.
      const owner = await isOwner(pubkey);
      const license = JSON.stringify({ pubkey, hw: data.hw_hash, owner, iat: Date.now() });
      return json({ ok: true, revoked: false, owner, license, licenseSig: await sign(license) });
    }

    case "owner": {
      if (!(await isOwner(pubkey))) return json({ error: "forbidden" }, 403);
      const action = String(p.action ?? "");
      switch (action) {
        case "requests": {
          const { data } = await db.from("requests").select("id, name, message, created_at, ip, hw_hash").eq("status", "pending").order("created_at", { ascending: false }).limit(100);
          return json((data ?? []).map((r) => ({ id: r.id, name: r.name, message: r.message, at: Date.parse(r.created_at), ip: r.ip ?? "", hw: (r.hw_hash ?? "").slice(0, 12) })));
        }
        case "issue": {
          const label = clean(p.label, 80);
          const code = newCode();
          await db.from("codes").insert({ label, code_hash: await sha256(normalizeCode(code)) });
          if (p.requestId) await db.from("requests").update({ status: "approved" }).eq("id", String(p.requestId));
          return json({ code });
        }
        case "decline": {
          await db.from("requests").update({ status: "declined" }).eq("id", String(p.id));
          return json({ ok: true });
        }
        case "codes": {
          const { data } = await db.from("codes").select("id, label, created_at, used_at, revoked").eq("is_owner", false).eq("revoked", false).order("created_at", { ascending: false }).limit(200);
          return json((data ?? []).map((c) => ({ id: c.id, label: c.label, created: Date.parse(c.created_at), used: !!c.used_at })));
        }
        case "revoke_code": {
          await db.from("codes").update({ revoked: true }).eq("id", String(p.id));
          return json({ ok: true });
        }
        case "devices": {
          const [{ data: devs }, { data: blocked }] = await Promise.all([
            db.from("devices").select("id, name, label, display_name, activated_at, last_seen, revoked, pubkey, hw_hash, last_ip").eq("revoked", false).order("last_seen", { ascending: false }).limit(200),
            db.from("hw_attempts").select("hw_hash, name, updated_at, last_ip").eq("blocked", true).limit(200),
          ]);
          const own = (await db.from("owner").select("pubkey").eq("id", 1).maybeSingle()).data?.pubkey;
          return json([
            ...(blocked ?? []).map((b) => ({
              id: "hw:" + b.hw_hash, name: b.name || b.hw_hash.slice(0, 8), label: "", computer: b.name ?? "", displayName: "", ip: b.last_ip ?? "", hw: b.hw_hash.slice(0, 12),
              activated: 0, lastSeen: Date.parse(b.updated_at), blocked: true,
            })),
            ...(devs ?? []).filter((d) => d.pubkey !== own).map((d) => ({
              id: d.id, name: [d.label, d.name].filter(Boolean).join(" — "), label: d.label ?? "", computer: d.name ?? "", displayName: d.display_name ?? "",
              ip: d.last_ip ?? "", hw: (d.hw_hash ?? "").slice(0, 12), activated: Date.parse(d.activated_at), lastSeen: Date.parse(d.last_seen), blocked: false,
            })),
          ]);
        }
        case "revoke_device": {
          await db.from("devices").update({ revoked: true }).eq("id", String(p.id));
          return json({ ok: true });
        }
        case "unblock": {
          const id = String(p.id);
          if (id.startsWith("hw:")) await db.from("hw_attempts").update({ blocked: false, failures: 0 }).eq("hw_hash", id.slice(3));
          return json({ ok: true });
        }
        default:
          return json({ error: "unknown" }, 400);
      }
    }

    default:
      return json({ error: "unknown" }, 400);
  }
}

Deno.serve(async (req) => {
  try {
    return await handle(req);
  } catch (e) {
    console.error(e);
    return json({ error: "server" }, 500);
  }
});
