<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { availableLocales } from "@/i18n";
import { Globe, Check, Settings, X, Info, SlidersHorizontal, Github, Sun, Moon, Palette, RotateCcw } from "lucide-vue-next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { dotColor, resetDotColor, DEFAULT_DOT_COLOR } from "@/utils/uiSettings";

const GITHUB_URL = "https://github.com/unrealx766/BreezeRESP";

const { t, locale } = useI18n();

const visible = ref(false);
const activeTab = ref<"general" | "about">("general");

// Theme state
const THEME_KEY = "breezeresp-theme";
const currentTheme = ref<"light" | "dark">(
  (localStorage.getItem(THEME_KEY) as "light" | "dark") || "light"
);

function setTheme(theme: "light" | "dark") {
  currentTheme.value = theme;
  localStorage.setItem(THEME_KEY, theme);
  document.documentElement.classList.toggle("dark", theme === "dark");
}

function open() {
  visible.value = true;
}

function close() {
  visible.value = false;
}

const LOCALE_KEY = "breezeresp-locale";

function setLocale(code: string) {
  locale.value = code;
  localStorage.setItem(LOCALE_KEY, code);
}

const appVersion = __APP_VERSION__;

defineExpose({ open });
</script>

<template>
  <Teleport to="body">
    <Transition name="fade">
      <div v-if="visible" class="fixed inset-0 z-[10000] flex items-center justify-center">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/40" @click="close" />

        <!-- Dialog -->
        <div class="relative bg-bg-secondary rounded-xl shadow-2xl border border-border w-[520px] h-[380px] max-w-[90vw] animate-in overflow-hidden flex flex-col">
          <!-- Header -->
          <div class="flex items-center justify-between px-5 py-3.5 border-b border-border-light">
            <div class="flex items-center gap-2.5">
              <div class="w-8 h-8 rounded-lg bg-redis/10 flex items-center justify-center">
                <Settings :size="16" class="text-redis" />
              </div>
              <h3 class="text-sm font-semibold text-text-primary">{{ t("settings.title") }}</h3>
            </div>
            <button
              @click="close"
              class="w-7 h-7 rounded-lg flex items-center justify-center text-text-muted hover:bg-bg-hover hover:text-text-primary transition-colors"
            >
              <X :size="14" />
            </button>
          </div>

          <!-- Body: Sidebar + Content -->
          <div class="flex flex-1 min-h-0">
            <!-- Side Tabs -->
            <div class="w-[130px] shrink-0 bg-bg-primary/50 border-r border-border-light py-2 px-2 flex flex-col gap-0.5">
              <button
                @click="activeTab = 'general'"
                class="flex items-center gap-2 px-2.5 py-2 text-xs font-medium rounded-lg transition-colors"
                :class="activeTab === 'general'
                  ? 'bg-redis/10 text-redis'
                  : 'text-text-muted hover:bg-bg-hover hover:text-text-secondary'"
              >
                <SlidersHorizontal :size="13" />
                {{ t("settings.general") }}
              </button>
              <button
                @click="activeTab = 'about'"
                class="flex items-center gap-2 px-2.5 py-2 text-xs font-medium rounded-lg transition-colors"
                :class="activeTab === 'about'
                  ? 'bg-redis/10 text-redis'
                  : 'text-text-muted hover:bg-bg-hover hover:text-text-secondary'"
              >
                <Info :size="13" />
                {{ t("settings.about") }}
              </button>
            </div>

            <!-- Content -->
            <div class="flex-1 px-5 py-4 min-w-0 overflow-y-auto">
              <!-- General Tab -->
              <div v-if="activeTab === 'general'" class="space-y-5">
                <!-- Theme Setting -->
                <div>
                  <div class="flex items-center gap-2 mb-2.5">
                    <component :is="currentTheme === 'dark' ? Moon : Sun" :size="13" class="text-text-muted" />
                    <span class="text-xs font-medium text-text-primary">{{ t("settings.theme") }}</span>
                  </div>
                  <p class="text-[11px] text-text-muted mb-3 pl-[21px]">{{ t("settings.themeDesc") }}</p>
                  <div class="flex gap-2 pl-[21px]">
                    <button
                      @click="setTheme('light')"
                      class="flex items-center justify-between px-3 py-2 text-xs rounded-lg border transition-colors min-w-[100px]"
                      :class="currentTheme === 'light'
                        ? 'border-redis/40 bg-redis/5 text-redis font-medium'
                        : 'border-border bg-bg-secondary text-text-secondary hover:border-border-hover hover:bg-bg-hover'"
                    >
                      <div class="flex items-center gap-1.5">
                        <Sun :size="12" />
                        <span>{{ t("settings.themeLight") }}</span>
                      </div>
                      <Check v-if="currentTheme === 'light'" :size="12" />
                    </button>
                    <button
                      @click="setTheme('dark')"
                      class="flex items-center justify-between px-3 py-2 text-xs rounded-lg border transition-colors min-w-[100px]"
                      :class="currentTheme === 'dark'
                        ? 'border-redis/40 bg-redis/5 text-redis font-medium'
                        : 'border-border bg-bg-secondary text-text-secondary hover:border-border-hover hover:bg-bg-hover'"
                    >
                      <div class="flex items-center gap-1.5">
                        <Moon :size="12" />
                        <span>{{ t("settings.themeDark") }}</span>
                      </div>
                      <Check v-if="currentTheme === 'dark'" :size="12" />
                    </button>
                  </div>
                </div>

                <!-- Language Setting -->
                <div>
                  <div class="flex items-center gap-2 mb-2.5">
                    <Globe :size="13" class="text-text-muted" />
                    <span class="text-xs font-medium text-text-primary">{{ t("settings.language") }}</span>
                  </div>
                  <p class="text-[11px] text-text-muted mb-3 pl-[21px]">{{ t("settings.languageDesc") }}</p>
                  <div class="flex gap-2 pl-[21px]">
                    <button
                      v-for="loc in availableLocales"
                      :key="loc.code"
                      @click="setLocale(loc.code)"
                      class="flex items-center justify-between px-3 py-2 text-xs rounded-lg border transition-colors min-w-[120px]"
                      :class="locale === loc.code
                        ? 'border-redis/40 bg-redis/5 text-redis font-medium'
                        : 'border-border bg-bg-secondary text-text-secondary hover:border-border-hover hover:bg-bg-hover'"
                    >
                      <span>{{ loc.label }}</span>
                      <Check v-if="locale === loc.code" :size="12" />
                    </button>
                  </div>
                </div>

                <!-- Connection Dot Color -->
                <div>
                  <div class="flex items-center gap-2 mb-2.5">
                    <Palette :size="13" class="text-text-muted" />
                    <span class="text-xs font-medium text-text-primary">{{ t("settings.dotColor") }}</span>
                  </div>
                  <p class="text-[11px] text-text-muted mb-3 pl-[21px]">{{ t("settings.dotColorDesc") }}</p>
                  <div class="flex items-center gap-3 pl-[21px]">
                    <label class="relative w-8 h-8 rounded-lg border border-border cursor-pointer overflow-hidden hover:border-border-hover transition-colors">
                      <span class="absolute inset-0" :style="{ backgroundColor: dotColor }" />
                      <input
                        type="color"
                        v-model="dotColor"
                        class="absolute inset-0 opacity-0 cursor-pointer w-full h-full"
                      />
                    </label>
                    <span class="text-xs font-mono text-text-secondary">{{ dotColor }}</span>
                    <button
                      v-if="dotColor !== DEFAULT_DOT_COLOR"
                      @click="resetDotColor"
                      class="flex items-center gap-1 px-2 py-1 text-[11px] text-text-muted hover:text-text-secondary rounded-md hover:bg-bg-hover transition-colors"
                      :title="t('settings.dotColorReset')"
                    >
                      <RotateCcw :size="11" />
                      {{ t("settings.dotColorReset") }}
                    </button>
                  </div>
                </div>
              </div>

              <!-- About Tab -->
              <div v-if="activeTab === 'about'" class="space-y-4">
                <div class="flex flex-col items-center py-4 gap-3">
                  <img src="/breezeresp.svg" alt="BreezeRESP" class="w-14 h-14 rounded-2xl shadow-sm" />
                  <div class="text-center">
                    <p class="text-sm font-semibold text-text-primary">{{ t("app.title") }}</p>
                    <p class="text-xs text-text-muted mt-0.5">{{ t("app.subtitle") }}</p>
                  </div>
                  <span class="text-[11px] font-mono text-text-muted bg-bg-primary px-2.5 py-1 rounded-md">{{ appVersion }}</span>
                  <button
                    @click="openUrl(GITHUB_URL)"
                    class="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs text-text-secondary hover:bg-bg-hover hover:text-text-primary transition-colors"
                  >
                    <Github :size="14" />
                    <span>GitHub</span>
                  </button>
                  <p class="text-[11px] text-text-muted mt-1">{{ t("app.copyright") }}</p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.15s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
.animate-in {
  animation: dialog-in 0.2s ease-out;
}
@keyframes dialog-in {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(8px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
</style>
