import { useI18n } from "vue-i18n";

/**
 * Lightweight copy feedback — shows a small floating "已复制" tip
 * next to the trigger button that fades up and disappears.
 */
export function useCopyTip() {
  const { t } = useI18n();

  async function copyWithTip(text: string, el: HTMLElement | null | Event) {
    // Resolve trigger element
    const target =
      el instanceof HTMLElement
        ? el
        : el instanceof Event
          ? (el.currentTarget as HTMLElement)
          : null;

    // Write to clipboard with fallback
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }

    // Spawn floating tip
    if (!target) return;
    const rect = target.getBoundingClientRect();

    const tip = document.createElement("span");
    tip.textContent = t("common.copied");
    tip.className = "copy-tip-float";
    tip.style.left = `${rect.left + rect.width / 2}px`;
    tip.style.top = `${rect.top - 4}px`;
    document.body.appendChild(tip);

    setTimeout(() => tip.remove(), 900);
  }

  return { copyWithTip };
}
