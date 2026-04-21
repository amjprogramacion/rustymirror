import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import { useDuplicatesHistoryStore } from './store/duplicatesHistory'
import { useMetadataHistoryStore } from './store/metadataHistory'
import { useOrganizerHistoryStore } from './store/organizerHistory'

import './styles/tokens.css'
import './styles/base.css'

const app = createApp(App)
const pinia = createPinia()
app.use(pinia)

// Load persisted scan histories before mounting
const history = useDuplicatesHistoryStore()
const metadataHistory = useMetadataHistoryStore()
const organizerHistory = useOrganizerHistoryStore()
Promise.all([history.load(), metadataHistory.load(), organizerHistory.load()]).then(() => {
  history.checkFolderStatus()
  metadataHistory.checkFolderStatus()
  organizerHistory.checkFolderStatus()
}).finally(() => app.mount('#app'))
