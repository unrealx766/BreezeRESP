import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { router } from "./router";
import { i18n } from "./i18n";
import "./assets/main.css";

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.use(i18n);
app.mount("#app");

// Disable default context menu (custom context menu may be added later)
document.addEventListener("contextmenu", (e) => e.preventDefault());
