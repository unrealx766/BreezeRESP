import { useI18n } from "vue-i18n";
import { toast } from "./toast";

/**
 * Lightweight save feedback:
 *   - Success: floating "已保存" tip near the trigger element
 *   - Error: global toast notification
 */
export function useSaveTip() {
  const { t } = useI18n();

  /**
   * Execute a save function and show appropriate feedback.
   * @param saveFn  async function that returns true on success, false on failure
   * @param el      trigger element or event (for positioning the tip)
   * @param errorMsg optional custom error message
   */
  async function handleSave(
    saveFn: () => Promise<boolean>,
    el: HTMLElement | Event | null,
    errorMsg?: string
  ) {
    const target =
      el instanceof HTMLElement
        ? el
        : el instanceof Event
          ? (el.currentTarget as HTMLElement)
          : null;

    const ok = await saveFn();

    if (ok) {
      // Spawn floating success tip
      if (target) {
        const rect = target.getBoundingClientRect();
        const tip = document.createElement("span");
        tip.textContent = t("detail.saveSuccess");
        tip.className = "copy-tip-float";
        tip.style.left = `${rect.left + rect.width / 2}px`;
        tip.style.top = `${rect.top - 4}px`;
        document.body.appendChild(tip);
        setTimeout(() => tip.remove(), 900);
      }
    } else {
      // Show global error toast
      toast.error(errorMsg || t("detail.saveFailed"));
    }

    return ok;
  }

  return { handleSave };
}
