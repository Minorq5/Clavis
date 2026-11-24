// Переход к нужному месту приложения из поиска, уведомлений и других разделов.
export type Section = "snippets" | "notes" | "settings" | "guide" | "owner";
export const nav = $state<{
  section: Section;
  target: { kind: "note" | "shared" | "snippet"; id: string } | null;
  /** Папка или командный набор в разделе сниппетов: all, none, id папки или team:<id>. */
  snipFolder: string;
  folders: { id: string; name: string }[];
  sets: { id: string; title: string }[];
}>({ section: "notes", target: null, snipFolder: "all", folders: [], sets: [] });

export function go(section: Section, target: { kind: "note" | "shared" | "snippet"; id: string } | null = null) {
  nav.section = section;
  nav.target = target;
}
