import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";
import ConnectionPage from "@/views/ConnectionPage.vue";
import BrowserPage from "@/views/BrowserPage.vue";
import PipelinePage from "@/views/PipelinePage.vue";
import SandboxPage from "@/views/SandboxPage.vue";
import HistoryPage from "@/views/HistoryPage.vue";

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    name: "connections",
    component: ConnectionPage,
  },
  {
    path: "/browser",
    name: "browser",
    component: BrowserPage,
  },
  {
    path: "/pipeline",
    name: "pipeline",
    component: PipelinePage,
  },
  {
    path: "/sandbox",
    name: "sandbox",
    component: SandboxPage,
  },
  {
    path: "/history",
    name: "history",
    component: HistoryPage,
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
