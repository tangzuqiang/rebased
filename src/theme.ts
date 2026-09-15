import { getAppSetting, setAppSetting } from "./api";

export type ThemeName = "dark" | "light";

const SETTING_KEY = "theme";
const STORAGE_KEY = "rebased.theme";

export function currentTheme(): ThemeName {
  const raw = document.documentElement.dataset.theme || localStorage.getItem(STORAGE_KEY);
  return raw === "light" ? "light" : "dark";
}

export function applyTheme(theme: ThemeName) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
  localStorage.setItem(STORAGE_KEY, theme);
  window.dispatchEvent(new CustomEvent("rebased-theme", { detail: theme }));
}

export async function loadTheme() {
  const saved = await getAppSetting(SETTING_KEY).catch(() => null);
  if (saved === "light" || saved === "dark") {
    applyTheme(saved);
    return saved;
  }
  const local = localStorage.getItem(STORAGE_KEY);
  const theme: ThemeName = local === "light" ? "light" : "dark";
  applyTheme(theme);
  return theme;
}

export async function saveTheme(theme: ThemeName) {
  applyTheme(theme);
  await setAppSetting(SETTING_KEY, theme);
}

export function terminalTheme(theme: ThemeName = currentTheme()) {
  if (theme === "light") {
    return {
      background: "#ffffff",
      foreground: "#1f2328",
      cursor: "#1f2328",
      selectionBackground: "#d6e4ff",
    };
  }
  return {
    background: "#1e1f22",
    foreground: "#dfe1e5",
    cursor: "#dfe1e5",
    selectionBackground: "#2e436e",
  };
}
