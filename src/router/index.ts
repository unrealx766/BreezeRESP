import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

const routes: RouteRecordRaw[] = [
  {
    path: "/",
    name: "connections",
    component: () => import("@/views/ConnectionPage.vue"),
  },
  {
    path: "/browser",
    name: "browser",
    component: () => import("@/views/BrowserPage.vue"),
  },
  {
    path: "/pipeline",
    name: "pipeline",
    component: () => import("@/views/PipelinePage.vue"),
  },
  {
    path: "/sandbox",
    name: "sandbox",
    component: () => import("@/views/SandboxPage.vue"),
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
