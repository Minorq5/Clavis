import { t } from "./i18n.svelte";

export function sharedError(e: unknown): string {
  const s = String(e);
  if (s === "network") return t("sh.errOffline");
  if (s === "link") return t("sh.errLink");
  if (s === "invite") return t("sh.errInvite");
  if (s === "already_member") return t("sh.errMember");
  return t("toast.error", { e: s });
}
