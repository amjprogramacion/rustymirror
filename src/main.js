import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { useHistoryStore } from './store/history'

import './styles/tokens.css'
import './styles/base.css'

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)

// Load persisted scan history before mounting
const history = useHistoryStore()
history.load().finally(() => app.mount('#app'))
