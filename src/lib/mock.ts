// Данные для просмотра интерфейса в обычном браузере, без ядра.
// Общие заметки здесь имитируются между вкладками браузера через BroadcastChannel,
// чтобы проверить совместное редактирование без сервера.
import * as Y from "yjs";
import type { ClipItem, Folder, Note, Settings, Snippet, Stage } from "./api";

const plain = (d: any): string => (!d ? "" : d.text ?? (d.content ?? []).map(plain).join(" "));
const noteMeta = new Map<string, { folder: string | null; tags: string[]; pinned: boolean; secret: boolean; trashed: number }>();
const metaOf = (id: string) => noteMeta.get(id) ?? { folder: null, tags: [], pinned: false, secret: false, trashed: 0 };
const meta = (n: Note) => { const m = metaOf(n.id); return { id: n.id, title: n.title, updated: n.updated, text: m.secret ? "" : plain(n.doc), ...m }; };

const params = new URLSearchParams(location.search);
const min = 60_000;
const now = Date.now();

const thumb = (() => {
  const c = document.createElement("canvas");
  c.width = 320; c.height = 200;
  const g = c.getContext("2d")!;
  g.fillStyle = "#23262b"; g.fillRect(0, 0, 320, 200);
  g.fillStyle = "#2f343b"; g.fillRect(0, 0, 320, 28);
  g.fillStyle = "#3d4450"; for (let i = 0; i < 6; i++) g.fillRect(18, 48 + i * 22, 120 + ((i * 53) % 140), 10);
  g.fillStyle = "#8aa0b8"; g.fillRect(230, 48, 70, 70);
  return c.toDataURL();
})();

let history: ClipItem[] = [
  { id: "h1", kind: "text", text: "Здравствуйте! Макет главной готов, ссылка на Figma ниже. Посмотрите, пожалуйста, до пятницы — после этого перейду к адаптиву.", at: now - 2 * min, pinned: false, source: "Telegram" },
  { id: "h2", kind: "text", text: '{"bot":"orders","token_env":"BOT_TOKEN","admins":[48211,90233],"locale":"ru"}', at: now - 9 * min, pinned: false, source: "Visual Studio Code" },
  { id: "h3", kind: "image", image: thumb, imageSize: [1920, 1080], at: now - 14 * min, pinned: false, source: "Ножницы" },
  { id: "h4", kind: "text", text: "Статья 45. Договор считается заключённым, если между сторонами достигнуто соглашение по всем существенным условиям.", at: now - 38 * min, pinned: true, source: "Microsoft Edge" },
  { id: "h5", kind: "files", files: ["D:\\work\\umbrella\\banner_v3.psd", "D:\\work\\umbrella\\banner_v3.png"], at: now - 65 * min, pinned: false, source: "Проводник" },
  { id: "h6", kind: "text", text: "ghbdtn? rfr ltkf c ghjtrnjv", at: now - 90 * min, pinned: false, source: "Discord" },
  { id: "h7", kind: "text", text: "https://github.com/kenzie-dev/portfolio", at: now - 3 * 60 * min, pinned: false, source: "Microsoft Edge" },
  { id: "h8", kind: "text", text: "#151514", at: now - 5 * 60 * min, pinned: false, source: "Figma" },
  { id: "h9", kind: "text", text: "git commit -m \"fix: adaptive header on 360px\"", at: now - 26 * 60 * min, pinned: false, source: "Терминал" },
];

let folders: Folder[] = [
  { id: "f1", name: "Клиенты" },
  { id: "f2", name: "Учёба" },
  { id: "f3", name: "Код" },
];

let snippets: Snippet[] = [
  { id: "s1", title: "Ответ на новый заказ", body: "Здравствуйте, {имя}! Спасибо за обращение. Посмотрю задачу сегодня и до {срок} пришлю оценку по срокам и стоимости.", folder: "f1", tags: ["клиент", "шаблон"], abbr: "заказ", updated: now - 3 * 60 * min },
  { id: "s2", title: "Реквизиты для оплаты", body: "Оплата по счёту, 50% предоплата.\nДата выставления: {дата}", folder: "f1", tags: ["оплата"], abbr: "опл", updated: now - 2 * 24 * 60 * min },
  { id: "s3", title: "Подпись", body: "Кензи\nвеб-дизайн и разработка", folder: null, tags: [], abbr: "подп", updated: now - 5 * 24 * 60 * min },
  { id: "s4", title: "Состав правонарушения", body: "Объект, объективная сторона, субъект, субъективная сторона.", folder: "f2", tags: ["право"], abbr: "", updated: now - 24 * 60 * min },
  { id: "s5", title: "Обработчик команды бота", body: "bot.command('{команда}', async (ctx) => {\n  await ctx.reply('{ответ}');\n});", folder: "f3", tags: ["telegram", "js"], abbr: "cmd", updated: now - 4 * 24 * 60 * min },
];

let notes: Note[] = [
  {
    id: "n1", title: "Теория государства и права", updated: now - 60 * min,
    doc: { type: "doc", content: [
      { type: "heading", attrs: { level: 1 }, content: [{ type: "text", text: "Теория государства и права" }] },
      { type: "paragraph", content: [{ type: "text", text: "Государство — " }, { type: "text", marks: [{ type: "bold" }], text: "политическая организация общества" }, { type: "text", text: ", обладающая суверенитетом и аппаратом принуждения." }] },
      { type: "heading", attrs: { level: 2 }, content: [{ type: "text", text: "Признаки" }] },
      { type: "bulletList", content: ["Публичная власть", "Территория", "Суверенитет", "Налоги"].map((t) => ({ type: "listItem", content: [{ type: "paragraph", content: [{ type: "text", text: t }] }] })) },
      { type: "heading", attrs: { level: 2 }, content: [{ type: "text", text: "К семинару" }] },
      { type: "taskList", content: [["Прочитать главу 3", true], ["Выписать определения", false], ["Подготовить вопрос про формы правления", false]].map(([t, c]) => ({ type: "taskItem", attrs: { checked: c }, content: [{ type: "paragraph", content: [{ type: "text", text: t as string }] }] })) },
    ] },
  },
  { id: "n2", title: "Umbrella — правки баннеров", updated: now - 26 * 60 * min, doc: { type: "doc", content: [{ type: "heading", attrs: { level: 1 }, content: [{ type: "text", text: "Umbrella — правки баннеров" }] }, { type: "paragraph", content: [{ type: "text", text: "Сделать светлее фон и крупнее логотип." }] }] } },
  { id: "n3", title: "Идеи для портфолио", updated: now - 3 * 24 * 60 * min, doc: { type: "doc", content: [{ type: "heading", attrs: { level: 1 }, content: [{ type: "text", text: "Идеи для портфолио" }] }] } },
];

let noteFolders: Folder[] = [{ id: "nf1", name: "Учёба" }, { id: "nf2", name: "Работа" }];
let journal = [
  { at: now - 3 * min, kind: "unlock_pin", detail: "DESKTOP-MIRO" },
  { at: now - 3 * 60 * min, kind: "lock_idle", detail: "" },
  { at: now - 5 * 60 * min, kind: "unlock_fail", detail: "DESKTOP-MIRO" },
  { at: now - 26 * 60 * min, kind: "setup", detail: "DESKTOP-MIRO" },
];
let hello = false;
const uses: Record<string, { count: number; last: number }> = { s1: { count: 42, last: now - 30 * min }, s3: { count: 17, last: now - 3 * 60 * min }, s5: { count: 3, last: now - 6 * 24 * 60 * min } };
const consentSet = new Set<string>();
let versions: { id: string; note: string; at: number; size: number; doc: unknown }[] = [];
let reminders: { id: string; note: string; text: string; at: number; fired: boolean }[] = [];
let templates: { id: string; title: string; doc: unknown }[] = [];
const secrets = new Map<string, string>();
const unlocked = new Set<string>();
const teamSnips = [
  { id: "t1", setId: "ts1", setTitle: "Veritas", title: "Приветствие в RP", body: "Здравствуйте, {имя}. Чем могу помочь?", tags: ["rp"], abbr: "прив" },
  { id: "t2", setId: "ts1", setTitle: "Veritas", title: "Сбор состава", body: "Сбор всего состава через 10 минут у штаба.", tags: ["rp"], abbr: "сбор" },
];

let settings: Settings = {
  lang: "ru",
  hotkey: "Ctrl+Shift+V",
  mode: "popup",
  historyLimit: 100,
  enterAction: "paste",
  expander: { enabled: true, prefix: ";" },
  ignorePasswordManagers: true,
  excludedApps: ["KeePassXC.exe", "1Password.exe"],
  autostart: true,
  lockAfterMinutes: 30,
  displayName: params.get("name") ?? "",
  legalAccepted: params.has("consent") ? "" : "2026-11-24.2",
  starAsked: !params.has("star"),
  quickHotkey: "Ctrl+Alt+N",
  theme: (params.get("theme") as "dark" | "light") ?? "dark",
};

let stage: Stage = (params.get("stage") as Stage) ?? "ready";
let attempts = 15;
let paused = false;

// ——— имитация общих заметок ———
const bus = typeof BroadcastChannel !== "undefined" ? new BroadcastChannel("clavis-mock") : null;
const handlers = new Map<string, Set<(p: unknown) => void>>();
export function mockListen(event: string, cb: (p: unknown) => void): () => void {
  if (!handlers.has(event)) handlers.set(event, new Set());
  handlers.get(event)!.add(cb);
  return () => handlers.get(event)?.delete(cb);
}
const emit = (event: string, p: unknown) => handlers.get(event)?.forEach((h) => h(p));
if (bus) bus.onmessage = (e) => emit(e.data.event, e.data.payload);
const me = crypto.randomUUID();
const store = {
  list: (): any[] => JSON.parse(localStorage.getItem("mock-shared") ?? "[]"),
  setList: (l: any[]) => localStorage.setItem("mock-shared", JSON.stringify(l)),
  ups: (id: string): string[] => JSON.parse(localStorage.getItem("mock-ups-" + id) ?? "[]"),
  addUp: (id: string, u: string) => localStorage.setItem("mock-ups-" + id, JSON.stringify([...store.ups(id), u])),
};
const b64d = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
const b64e = (u: Uint8Array) => { let s = ""; for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode(...u.subarray(i, i + 0x8000)); return btoa(s); };
const images = new Map<string, string>();

export async function mock<T>(cmd: string, a: Record<string, any>): Promise<T> {
  const r = (v: unknown) => v as T;
  switch (cmd) {
    case "status": return r({ stage, owner: params.has("owner"), attemptsLeft: attempts });
    case "activate":
      if (/^[A-Z0-9]{4}(-[A-Z0-9]{4}){5}$/.test(a.code)) { stage = "setup"; return r({ ok: true, attemptsLeft: attempts }); }
      attempts--; return r({ ok: false, attemptsLeft: attempts, error: "wrong" });
    case "request_access": return r(undefined);
    case "set_pin": return r("orbit velvet canyon hollow ember lunar quartz meadow pilot ribbon timber anchor glide harbor nectar violin saddle frost cobalt mirror plaza spirit walnut zebra".split(" "));
    case "confirm_recovery": stage = "ready"; return r(undefined);
    case "unlock": if (a.pin === "2468") { stage = "ready"; return r({ ok: true, attemptsLeft: attempts }); } attempts--; return r({ ok: false, attemptsLeft: attempts });
    case "history_list": return r(history);
    case "history_pin": history = history.map((h) => (h.id === a.id ? { ...h, pinned: a.pinned } : h)); return r(undefined);
    case "history_delete": history = history.filter((h) => h.id !== a.id); return r(undefined);
    case "history_clear": history = history.filter((h) => h.pinned); return r(undefined);
    case "history_pause": paused = a.paused; return r(undefined);
    case "history_paused": return r(paused);
    case "folders_list": return r(folders);
    case "folder_save": { const f = a.folder.id ? a.folder : { ...a.folder, id: crypto.randomUUID() }; folders = [...folders.filter((x) => x.id !== f.id), f]; return r(f); }
    case "folder_delete": folders = folders.filter((f) => f.id !== a.id); snippets = snippets.map((s) => (s.folder === a.id ? { ...s, folder: null } : s)); return r(undefined);
    case "snippets_list": return r(snippets);
    case "snippet_save": { const s = { ...a.snippet, id: a.snippet.id || crypto.randomUUID(), updated: Date.now() }; snippets = [s, ...snippets.filter((x) => x.id !== s.id)]; return r(s); }
    case "snippet_delete": snippets = snippets.filter((s) => s.id !== a.id); return r(undefined);
    case "notes_list": return r(notes.map(meta));
    case "note_get": return r(metaOf(a.id).secret && !unlocked.has(a.id) ? { secret: true } : notes.find((n) => n.id === a.id)?.doc ?? null);
    case "note_save": {
      const n = { ...a.note, id: a.note.id || crypto.randomUUID(), updated: Date.now() };
      const old = notes.find((x) => x.id === n.id);
      if (!noteMeta.has(n.id)) noteMeta.set(n.id, { ...metaOf(n.id), folder: a.folder ?? null });
      if (old && !metaOf(n.id).secret && JSON.stringify(old.doc) !== JSON.stringify(n.doc) && !versions.some((v) => v.note === n.id && Date.now() - v.at < 600_000))
        versions = [{ id: crypto.randomUUID(), note: n.id, at: old.updated, size: JSON.stringify(old.doc).length, doc: old.doc }, ...versions];
      notes = [n, ...notes.filter((x) => x.id !== n.id)];
      return r(meta(n));
    }
    case "journal_list": return r(journal);
    case "hello_status": return r({ available: true, enabled: hello });
    case "hello_enable": hello = true; journal = [{ at: Date.now(), kind: "hello_on", detail: "" }, ...journal]; return r(undefined);
    case "hello_disable": hello = false; journal = [{ at: Date.now(), kind: "hello_off", detail: "" }, ...journal]; return r(undefined);
    case "unlock_hello": stage = "ready"; return r(true);
    case "ocr_item": await new Promise((f) => setTimeout(f, 500)); return r("Макет главной\nШапка · Каталог · Контакты\nКупить сейчас");
    case "paste_image": return r(undefined);
    case "consent_give": consentSet.add(a.service); return r(undefined);
    case "consent_revoke": consentSet.delete(a.service); return r(undefined);
    case "consents": return r([...consentSet]);
    case "translate":
      if (!consentSet.has("translate")) throw "consent:translate";
      await new Promise((f) => setTimeout(f, 400));
      return r(a.to === "en" ? "Hello! The home page layout is ready, the Figma link is below." : a.to === "bg" ? "Здравейте! Макетът на началната страница е готов." : "Здравствуйте! Макет главной готов.");
    case "shorten": if (!consentSet.has("shorten")) throw "consent:shorten"; return r("https://clck.ru/3FkQ9x");
    case "note_folders": return r(noteFolders);
    case "note_folder_save": { const f = a.folder.id ? a.folder : { ...a.folder, id: crypto.randomUUID() }; noteFolders = [...noteFolders.filter((x) => x.id !== f.id), f]; return r(f); }
    case "note_folder_delete": noteFolders = noteFolders.filter((f) => f.id !== a.id); for (const [k, m] of noteMeta) if (m.folder === a.id) noteMeta.set(k, { ...m, folder: null }); return r(undefined);
    case "note_meta_set": { noteMeta.set(a.id, { ...metaOf(a.id), folder: a.folder, tags: a.tags, pinned: a.pinned }); return r(meta(notes.find((n) => n.id === a.id)!)); }
    case "shared_pin": store.setList(store.list().map((x) => (x.id === a.id ? { ...x, pinned: a.pinned } : x))); return r(undefined);
    case "note_unlock": if (secrets.get(a.id) !== a.password) throw "wrong_password"; unlocked.add(a.id); return r(notes.find((n) => n.id === a.id)?.doc);
    case "note_set_secret": {
      const n = notes.find((x) => x.id === a.id)!;
      if (a.password) { if (a.password.length < 4) throw "password_short"; secrets.set(a.id, a.password); unlocked.add(a.id); versions = versions.filter((v) => v.note !== a.id); }
      else { secrets.delete(a.id); unlocked.delete(a.id); }
      n.doc = a.doc;
      noteMeta.set(a.id, { ...metaOf(a.id), secret: !!a.password });
      return r(meta(n));
    }
    case "note_lock_secret": unlocked.delete(a.id); return r(undefined);
    case "versions_list": return r(versions.filter((v) => v.note === a.note).map(({ doc, ...m }) => m));
    case "version_get": return r({ kind: "json", doc: versions.find((v) => v.id === a.id)?.doc });
    case "reminders_set": reminders = [...reminders.filter((x) => x.note !== a.note), ...a.items.map((i: any) => ({ id: crypto.randomUUID(), note: a.note, text: i.text, at: i.at, fired: false }))]; return r(undefined);
    case "reminders_list": return r(reminders);
    case "templates_list": return r(templates);
    case "template_save": { const tp = { id: crypto.randomUUID(), title: a.title, doc: a.doc }; templates = [...templates, tp]; return r(tp); }
    case "template_delete": templates = templates.filter((x) => x.id !== a.id); return r(undefined);
    case "team_snippets": return r(teamSnips);
    case "snippet_used": uses[a.id] = { count: (uses[a.id]?.count ?? 0) + 1, last: Date.now() }; return r(undefined);
    case "snippet_uses": return r(uses);
    case "history_image": return r(thumb);
    case "backup_export": return r("D:\\Clavis 2026-11-24.clavisbak");
    case "backup_import": return r(true);
    case "quick_note": { const n = { id: crypto.randomUUID(), title: a.text.split("\n")[0], updated: Date.now(), doc: { type: "doc", content: [{ type: "heading", attrs: { level: 1 }, content: [{ type: "text", text: a.text.split("\n")[0] }] }] } }; notes = [n, ...notes]; return r(undefined); }
    case "export_text": case "export_pdf": return r("D:\\Документы\\" + a.name + "." + (cmd === "export_pdf" ? "pdf" : a.ext));
    case "note_delete": noteMeta.set(a.id, { ...metaOf(a.id), trashed: Date.now(), pinned: false }); return r(undefined);
    case "note_restore": noteMeta.set(a.id, { ...metaOf(a.id), trashed: 0 }); return r(meta(notes.find((n) => n.id === a.id)!));
    case "note_purge": notes = notes.filter((n) => n.id !== a.id); noteMeta.delete(a.id); return r(undefined);
    case "trash_empty": for (const [k, m] of noteMeta) if (m.trashed) { notes = notes.filter((n) => n.id !== k); noteMeta.delete(k); } return r(undefined);
    case "settings_get": return r(structuredClone(settings));
    case "settings_set": settings = a.settings; return r(undefined);
    case "owner_requests": return r([{ id: "q1", name: "Тривия", message: "Для общих заметок команды", at: now - 40 * min, ip: "94.155.18.204", hw: "3f9a1c0be27d" }, { id: "q2", name: "Ринусик", message: "", at: now - 5 * 60 * min, ip: "188.254.172.41", hw: "a81d55e0c4f2" }]);
    case "owner_codes": return r([{ id: "c1", label: "Кензи", created: now - 2 * 24 * 60 * min, used: true }, { id: "c2", label: "Клиент — сайт кафе", created: now - 60 * min, used: false }]);
    case "owner_devices": return r([{ id: "d1", name: "Кензи — DESKTOP-7KQ2", label: "Кензи", computer: "DESKTOP-7KQ2", displayName: "Кензи", ip: "212.39.89.17", hw: "c07e2b91fa36", activated: now - 2 * 24 * 60 * min, lastSeen: now - 10 * min, blocked: false }, { id: "d2", name: "LAPTOP-91AF", computer: "LAPTOP-91AF", ip: "46.10.205.3", hw: "5be8d1a07c94", activated: 0, lastSeen: now - 3 * 60 * min, blocked: true }]);
    case "owner_issue_code": return r("K7QM-2XVD-9HTP-W4NC-R8JF-6BLZ");
    case "copy_text": return r(undefined);
    case "recover": stage = "ready"; return r({ ok: a.words.length === 24 });
    case "shared_list": return r(store.list());
    case "shared_create": { const m = { id: crypto.randomUUID(), title: a.title, role: "owner", updated: Date.now(), kind: a.kind ?? "note", pinned: false }; store.setList([m, ...store.list()]); return r(m); }
    case "shared_open": {
      const ups = store.ups(a.id).map(b64d);
      const state = ups.length ? Y.mergeUpdates(ups) : Y.encodeStateAsUpdate(new Y.Doc());
      const m = store.list().find((x) => x.id === a.id);
      return r({ state: b64e(state), role: m?.role ?? "editor", me, name: settings.displayName || "Олег Джайден", online: true });
    }
    case "shared_close": return r(undefined);
    case "shared_push": store.addUp(a.id, a.update); bus?.postMessage({ event: "shared:update", payload: { id: a.id, data: a.update } }); return r(undefined);
    case "shared_awareness": bus?.postMessage({ event: "shared:awareness", payload: { id: a.id, data: a.data } }); return r(undefined);
    case "shared_title": store.setList(store.list().map((x) => (x.id === a.id ? { ...x, title: a.title, updated: Date.now() } : x))); return r(undefined);
    case "shared_invite": return r(`clavis:join/${a.id}/q7Xk2vR9mN4pL8sT1wY6zB3cF5hJ0dG2aE9uK4oI7rQ`);
    case "shared_join": throw "invite";
    case "shared_members": return r([{ device: me, name: settings.displayName || "Олег Джайден", role: "owner", me: true }, { device: "d-trivia", name: "Тривия", role: "editor", me: false }, { device: "d-bruno", name: "Ринусик", role: "editor", me: false }]);
    case "shared_remove": return r(undefined);
    case "shared_leave": store.setList(store.list().filter((x) => x.id !== a.id)); return r(undefined);
    case "shared_image_put": { const id = crypto.randomUUID(); images.set(id, a.dataUrl); return r("clavis-img:" + id); }
    case "shared_image_get": return r(images.get(a.blob) ?? "");
    case "shared_online": return r(true);
    default: return r(null);
  }
}
