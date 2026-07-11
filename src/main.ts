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
app.mount("#app");

// Register connection guard for centralized IPC validation
import { useConnectionStore } from "./stores/connectionStore";
registerConnectionGuard(() => {
  const connStore = useConnectionStore();
  const conn = connStore.activeConnection;
  if (!conn || conn.status !== "connected") {
    throw new Error("Not connected");
  }
});

// Disable default context menu (custom context menu may be added later)
document.addEventListener("contextmenu", (e) => e.preventDefault());
