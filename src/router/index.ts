import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'
import HomeView from '../views/HomeView.vue'

// Screens other than the home screen are loaded on first visit, keeping startup light.
export const routes: RouteRecordRaw[] = [
  { path: '/', name: 'home', component: HomeView },
  { path: '/image', name: 'image', component: () => import('../views/ImageView.vue') },
  { path: '/pdf', name: 'pdf', component: () => import('../views/PdfView.vue') },
  { path: '/video', name: 'video', component: () => import('../views/VideoView.vue') },
  { path: '/audio', name: 'audio', component: () => import('../views/AudioView.vue') },
  { path: '/tools', name: 'tools', component: () => import('../views/ToolsView.vue') },
  { path: '/settings', name: 'settings', component: () => import('../views/SettingsView.vue') },
  { path: '/about', name: 'about', component: () => import('../views/AboutView.vue') },
  { path: '/:pathMatch(.*)*', redirect: '/' },
]

export function setupRouter() {
  return createRouter({
    history: createWebHashHistory(),
    routes,
  })
}
