import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { setupI18n } from './i18n'
import { setupRouter } from './router'
import './assets/main.css'

createApp(App).use(createPinia()).use(setupRouter()).use(setupI18n()).mount('#app')
