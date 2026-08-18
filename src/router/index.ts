import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";
import ConnectionPage from "@/views/ConnectionPage.vue";
import BrowserPage from "@/views/BrowserPage.vue";
import PipelinePage from "@/views/PipelinePage.vue";
import SandboxPage from "@/views/SandboxPage.vue";
import HistoryPage from "@/views/HistoryPage.vue";
import PubSubPage from "@/views/PubSubPage.vue";
import MonitorPage from "@/views/MonitorPage.vue";
import StreamsPage from "@/views/StreamsPage.vue";
import SearchPage from "@/views/SearchPage.vue";

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
  {
    path: "/pubsub",
    name: "pubsub",
    component: PubSubPage,
  },
  {
    path: "/monitor",
    name: "monitor",
    component: MonitorPage,
  },
  {
    // Legacy entry — slow queries now live inside the monitoring center
    path: "/slowlog",
    redirect: "/monitor",
  },
  {
    path: "/streams",
    name: "streams",
    component: StreamsPage,
  },
  {
    path: "/search",
    name: "search",
    component: SearchPage,
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
