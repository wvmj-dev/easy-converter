import { createRouter, createWebHashHistory, type RouteRecordRaw } from 'vue-router'
import HomeView from '../views/HomeView.vue'

// Screens other than the home screen are loaded on first visit, keeping startup light.
export const routes: RouteRecordRaw[] = [
  { path: '/', name: 'home', component: HomeView },
  { path: '/image', name: 'image', component: () => import('../views/ImageView.vue') },
  { path: '/pdf', name: 'pdf', component: () => import('../views/PdfView.vue') },
  { path: '/video', name: 'video', component: () => import('../views/VideoView.vue') },
  { path: '/audio', name: 'audio', component: () => import('../views/AudioView.vue') },
  { path: '/document', name: 'document', component: () => import('../views/DocumentView.vue') },
  {
    path: '/spreadsheet',
    name: 'spreadsheet',
    component: () => import('../views/SpreadsheetView.vue'),
  },
  {
    path: '/presentation',
    name: 'presentation',
    component: () => import('../views/PresentationView.vue'),
  },
  { path: '/subtitle', name: 'subtitle', component: () => import('../views/SubtitleView.vue') },
  { path: '/ebook', name: 'ebook', component: () => import('../views/EbookView.vue') },
  { path: '/archive', name: 'archive', component: () => import('../views/ArchiveView.vue') },
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
