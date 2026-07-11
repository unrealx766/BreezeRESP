import { createI18n } from "vue-i18n";
import zhCN from "./zh-CN.json";
import en from "./en.json";

const SUPPORTED_LOCALES = ["zh-CN", "en"] as const;

/** Detect system language, fallback to en */
function detectLocale(): string {
  // Prefer persisted user choice
  const saved = localStorage.getItem("breezeresp-locale");
  if (saved && SUPPORTED_LOCALES.includes(saved as any)) return saved;

  // Detect from OS / browser language
  const nav = navigator.language; // e.g. "zh-CN", "en-US", "zh-TW"
  if (SUPPORTED_LOCALES.includes(nav as any)) return nav;
  // Match language prefix (e.g. "zh" -> "zh-CN", "en" -> "en")
  const prefix = nav.split("-")[0];
  if (prefix === "zh") return "zh-CN";
  return "en";
}

export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: "en",
  messages: {
    "zh-CN": zhCN,
    en,
  },
});

export const availableLocales = [
  { code: "zh-CN", label: "中文" },
  { code: "en", label: "English" },
];
