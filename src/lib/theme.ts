// Применяет тему окна: тёмную, светлую или как в Windows.
let media: MediaQueryList | null = null;
let current = "dark";

function paint() {
  const light = current === "light" || (current === "system" && !!media && !media.matches);
  document.documentElement.dataset.theme = light ? "light" : "dark";
}

export function applyTheme(theme: string) {
  current = theme || "dark";
  if (!media && typeof window !== "undefined") {
    media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", paint);
  }
  paint();
}
