import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'
import HomeView from '../views/HomeView.vue'
import SettingsView from '../views/SettingsView.vue'
import ToolsView from '../views/ToolsView.vue'

export const routes: RouteRecordRaw[] = [
  { path: '/', name: 'home', component: HomeView },
  { path: '/tools', name: 'tools', component: ToolsView },
  { path: '/settings', name: 'settings', component: SettingsView },
]

export function setupRouter() {
  return createRouter({
    history: createWebHashHistory(),
    routes,
  })
}
