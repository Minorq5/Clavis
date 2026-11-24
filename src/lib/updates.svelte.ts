// Автообновление: Clavis сам спрашивает у сервера, нет ли новой версии, и ставит её по согласию.
import { inTauri } from "./api";

type Found = { version: string; notes: string; install: (onProgress: (p: number) => void) => Promise<void> };
export const updates = $state<{ checking: boolean; found: Found | null; progress: number | null; error: string; dismissed: boolean; checked: boolean }>({
  checking: false, found: null, progress: null, error: "", dismissed: false, checked: false,
});

export async function checkUpdates(): Promise<boolean> {
  if (!inTauri || updates.checking) return false;
  updates.checking = true;
  updates.error = "";
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const u = await check({ timeout: 15000 });
    updates.checked = true;
    if (!u) { updates.found = null; return false; }
    updates.found = {
      version: u.version,
      notes: u.body ?? "",
      install: async (onProgress) => {
        let total = 0, got = 0;
        await u.downloadAndInstall((e) => {
          if (e.event === "Started") total = e.data.contentLength ?? 0;
          else if (e.event === "Progress") { got += e.data.chunkLength; if (total) onProgress(Math.min(99, Math.round((got / total) * 100))); }
          else if (e.event === "Finished") onProgress(100);
        });
        const { relaunch } = await import("@tauri-apps/plugin-process");
        await relaunch();
      },
    };
    updates.dismissed = false;
    return true;
  } catch (e) {
    updates.error = String(e);
    return false;
  } finally {
    updates.checking = false;
  }
}

export async function installUpdate() {
  const f = updates.found;
  if (!f || updates.progress !== null) return;
  updates.progress = 0;
  updates.error = "";
  try {
    await f.install((p) => (updates.progress = p));
  } catch (e) {
    updates.error = String(e);
    updates.progress = null;
  }
}
