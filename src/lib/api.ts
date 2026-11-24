
export type ClipKind = "text" | "image" | "files";

export interface ClipItem {
  id: string;
  kind: ClipKind;
  text?: string;
  rich?: boolean;
  image?: string; // data URL миниатюры
  imageSize?: [number, number];
  files?: string[];
  source?: string;
  at: number;
  pinned: boolean;
}

export interface Folder { id: string; name: string }

export interface Snippet {
  id: string;
  title: string;
  body: string;
  folder: string | null;
  tags: string[];
  abbr: string;
  updated: number;
}

export interface NoteMeta {
  id: string;
  title: string;
  updated: number;
  text: string;
  folder: string | null;
  tags: string[];
  pinned: boolean;
  secret: boolean;
  trashed: number;
}

export interface JournalEntry { at: number; kind: string; detail: string }
export interface Reminder { id: string; note: string; text: string; at: number; fired: boolean }
export interface VersionMeta { id: string; note: string; at: number; size: number }
export interface Template { id: string; title: string; doc: unknown }
export interface TeamSnippet { id: string; setId: string; setTitle: string; title: string; body: string; tags: string[]; abbr: string }

export interface Note {
  id: string;
  title: string;
  doc: unknown; // документ редактора в JSON
  updated: number;
}

export type EnterAction = "paste" | "copy";
export type PickerMode = "popup" | "panel";

export interface Settings {
  lang: "ru" | "en";
  hotkey: string;
  mode: PickerMode;
  historyLimit: number;
  enterAction: EnterAction;
  expander: { enabled: boolean; prefix: string };
  ignorePasswordManagers: boolean;
  excludedApps: string[];
  autostart: boolean;
  lockAfterMinutes: number;
  displayName: string;
  legalAccepted: string;
  quickHotkey: string;
  theme: "dark" | "light" | "system";
  starAsked: boolean;
}

export interface SharedMeta { id: string; title: string; role: "owner" | "editor"; updated: number; kind: "note" | "snippets"; pinned: boolean }
export interface SharedOpened { state: string; role: "owner" | "editor"; me: string; name: string; online: boolean }
export interface SharedMember { device: string; name: string; role: "owner" | "editor"; me: boolean }

export type Stage = "activate" | "setup" | "locked" | "ready" | "blocked";

export interface Status {
  stage: Stage;
  owner: boolean;
  attemptsLeft: number;
}

export interface AccessRequest { id: string; name: string; message: string; at: number; ip?: string; hw?: string }
export interface Device { id: string; name: string; activated: number; lastSeen: number; blocked: boolean; label?: string; computer?: string; displayName?: string; ip?: string; hw?: string }
export interface IssuedCode { id: string; label: string; created: number; used: boolean }

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  const { mock } = await import("./mock");
  return mock<T>(cmd, args ?? {});
}

export const api = {
  status: () => call<Status>("status"),
  activate: (code: string) => call<{ ok: boolean; attemptsLeft: number; error?: string }>("activate", { code }),
  requestAccess: (name: string, message: string) => call<void>("request_access", { name, message }),
  setPin: (pin: string) => call<string[]>("set_pin", { pin }),
  confirmRecovery: () => call<void>("confirm_recovery"),
  unlock: (pin: string) => call<{ ok: boolean; attemptsLeft: number; reason?: "tpm_locked" | "guard_missing" }>("unlock", { pin }),
  recover: (words: string[], newPin: string) => call<{ ok: boolean }>("recover", { words, newPin }),
  lock: () => call<void>("lock"),

  history: () => call<ClipItem[]>("history_list"),
  historyPin: (id: string, pinned: boolean) => call<void>("history_pin", { id, pinned }),
  historyDelete: (id: string) => call<void>("history_delete", { id }),
  historyClear: () => call<void>("history_clear"),
  historyPause: (paused: boolean) => call<void>("history_pause", { paused }),
  historyPaused: () => call<boolean>("history_paused"),

  /** Скрывает окно, кладёт текст в буфер и вставляет в окно, которое было активно до вызова. */
  pasteText: (text: string, paste: boolean) => call<void>("paste_text", { text, paste }),
  pasteItem: (id: string, plain: boolean, paste: boolean) => call<void>("paste_item", { id, plain, paste }),
  copyText: (text: string) => call<void>("copy_text", { text }),
  hidePicker: () => call<void>("hide_picker"),
  openMain: (section?: string) => call<void>("open_main", { section }),

  folders: () => call<Folder[]>("folders_list"),
  folderSave: (f: Folder) => call<Folder>("folder_save", { folder: f }),
  folderDelete: (id: string) => call<void>("folder_delete", { id }),
  snippets: () => call<Snippet[]>("snippets_list"),
  snippetSave: (s: Snippet) => call<Snippet>("snippet_save", { snippet: s }),
  snippetDelete: (id: string) => call<void>("snippet_delete", { id }),
  snippetsExport: () => call<string | null>("snippets_export"),
  snippetsImport: () => call<number | null>("snippets_import"),

  notes: () => call<NoteMeta[]>("notes_list"),
  noteGet: (id: string) => call<unknown>("note_get", { id }),
  noteSave: (n: Note, folder?: string | null) => call<NoteMeta>("note_save", { note: n, folder: folder ?? null }),
  noteDelete: (id: string) => call<void>("note_delete", { id }),
  noteRestore: (id: string) => call<NoteMeta>("note_restore", { id }),
  notePurge: (id: string) => call<void>("note_purge", { id }),
  trashEmpty: () => call<void>("trash_empty"),
  noteImage: () => call<string | null>("note_pick_image"),
  noteExportDocx: (name: string, base64: string) => call<string | null>("note_export_docx", { name, base64 }),

  sharedList: () => call<SharedMeta[]>("shared_list"),
  sharedCreate: (title: string, kind: "note" | "snippets" = "note") => call<SharedMeta>("shared_create", { title, kind }),
  sharedOpen: (id: string) => call<SharedOpened>("shared_open", { id }),
  sharedClose: (id: string) => call<void>("shared_close", { id }),
  sharedPush: (id: string, update: string) => call<void>("shared_push", { id, update }),
  sharedAwareness: (id: string, data: string) => call<void>("shared_awareness", { id, data }),
  sharedTitle: (id: string, title: string) => call<void>("shared_title", { id, title }),
  sharedInvite: (id: string) => call<string>("shared_invite", { id }),
  sharedJoin: (link: string) => call<string>("shared_join", { link }),
  sharedMembers: (id: string) => call<SharedMember[]>("shared_members", { id }),
  sharedRemove: (id: string, device: string) => call<void>("shared_remove", { id, device }),
  sharedLeave: (id: string) => call<void>("shared_leave", { id }),
  sharedImagePut: (id: string, dataUrl: string) => call<string>("shared_image_put", { id, dataUrl }),
  sharedImageGet: (id: string, blob: string) => call<string>("shared_image_get", { id, blob }),
  sharedOnline: () => call<boolean>("shared_online"),

  journal: () => call<JournalEntry[]>("journal_list"),
  helloStatus: () => call<{ available: boolean; enabled: boolean }>("hello_status"),
  helloEnable: () => call<void>("hello_enable"),
  helloDisable: () => call<void>("hello_disable"),
  unlockHello: () => call<boolean>("unlock_hello"),
  ocrItem: (id: string) => call<string>("ocr_item", { id }),
  pasteImage: (dataUrl: string, paste: boolean) => call<void>("paste_image", { dataUrl, paste }),
  consentGive: (service: "translate" | "shorten") => call<void>("consent_give", { service }),
  consentRevoke: (service: "translate" | "shorten") => call<void>("consent_revoke", { service }),
  consents: () => call<string[]>("consents"),
  translate: (text: string, to: "ru" | "en" | "bg") => call<string>("translate", { text, to }),
  shorten: (url: string) => call<string>("shorten", { url }),
  noteFolders: () => call<Folder[]>("note_folders"),
  noteFolderSave: (f: Folder) => call<Folder>("note_folder_save", { folder: f }),
  noteFolderDelete: (id: string) => call<void>("note_folder_delete", { id }),
  noteMetaSet: (id: string, folder: string | null, tags: string[], pinned: boolean) => call<NoteMeta>("note_meta_set", { id, folder, tags, pinned }),
  sharedPin: (id: string, pinned: boolean) => call<void>("shared_pin", { id, pinned }),
  noteUnlock: (id: string, password: string) => call<unknown>("note_unlock", { id, password }),
  noteSetSecret: (id: string, password: string | null, doc: unknown) => call<NoteMeta>("note_set_secret", { id, password, doc }),
  noteLockSecret: (id: string) => call<void>("note_lock_secret", { id }),
  versions: (note: string) => call<VersionMeta[]>("versions_list", { note }),
  versionGet: (id: string) => call<{ kind: "json"; doc: unknown } | { kind: "yjs"; state: string }>("version_get", { id }),
  remindersSet: (note: string, items: { text: string; at: number }[]) => call<void>("reminders_set", { note, items }),
  reminders: () => call<Reminder[]>("reminders_list"),
  templates: () => call<Template[]>("templates_list"),
  templateSave: (title: string, doc: unknown) => call<Template>("template_save", { title, doc }),
  templateDelete: (id: string) => call<void>("template_delete", { id }),
  teamSnippets: () => call<TeamSnippet[]>("team_snippets"),
  exportText: (name: string, ext: "md" | "txt", content: string) => call<string | null>("export_text", { name, ext, content }),
  eraseEverything: () => call<void>("erase_everything"),
  snippetUsed: (id: string) => call<void>("snippet_used", { id }),
  snippetUses: () => call<Record<string, { count: number; last: number }>>("snippet_uses"),
  historyImage: (id: string) => call<string>("history_image", { id }),
  backupExport: (password: string) => call<string | null>("backup_export", { password }),
  backupImport: (password: string) => call<boolean>("backup_import", { password }),
  quickNote: (text: string) => call<void>("quick_note", { text }),
  quickHide: () => call<void>("quick_hide"),
  openGithub: () => (isTauri ? call<void>("open_github") : (window.open("https://github.com/Minorq5/clavis", "_blank"), Promise.resolve())),
  exportPdf: (name: string, html: string) => call<string | null>("export_pdf", { name, html }),

  settings: () => call<Settings>("settings_get"),
  settingsSave: (s: Settings) => call<void>("settings_set", { settings: s }),

  ownerRequests: () => call<AccessRequest[]>("owner_requests"),
  ownerIssue: (label: string, requestId?: string) => call<string>("owner_issue_code", { label, requestId: requestId ?? null }),
  ownerCodes: () => call<IssuedCode[]>("owner_codes"),
  ownerDevices: () => call<Device[]>("owner_devices"),
  ownerUnblock: (id: string) => call<void>("owner_unblock", { id }),
  ownerRevokeCode: (id: string) => call<void>("owner_revoke_code", { id }),
  ownerRevokeDevice: (id: string) => call<void>("owner_revoke_device", { id }),
  ownerDecline: (id: string) => call<void>("owner_decline", { id }),
};

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  if (!isTauri) {
    const { mockListen } = await import("./mock");
    return mockListen(event, cb as (p: unknown) => void);
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => cb(e.payload));
}

export const inTauri = isTauri;
