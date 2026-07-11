import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { router } from "./router";
import { i18n } from "./i18n";
import { registerConnectionGuard } from "./services/tauriApi";
import "./assets/main.css";

const app = createApp(App);
const pinia = createPinia();
app.use(pinia);
app.use(router);
app.use(i18n);

// Register connection guard for centralized IPC validation (before mount)
import { useConnectionStore } from "./stores/connectionStore";
registerConnectionGuard(() => {
  const connStore = useConnectionStore();
  const conn = connStore.activeConnection;
  if (!conn || conn.status !== "connected") {
    throw new Error("Not connected");
  }
});

app.mount("#app");

// Disable default context menu (custom context menu may be added later)
document.addEventListener("contextmenu", (e) => e.preventDefault());
