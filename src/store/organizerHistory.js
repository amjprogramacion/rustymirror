import { createHistoryStore, foldersKey } from './historyFactory'

export const useOrganizerHistoryStore = createHistoryStore({
  id: 'organizerHistory',
  historyKey: 'organizerScanHistory',
  logPrefix: 'organizerHistory',
  clearLabel: 'organizer history',

  // Each entry: { id, folders, date, durationMs, total, images, videos }
  async addEntry(folders, total, images, videos, durationMs) {
    const key = foldersKey(folders)
    const match = e => foldersKey(e.folders) === key
    const existing = this.entries.find(match)

    const entry = {
      id: existing?.id ?? Date.now(),
      folders: [...folders],
      date: existing?.date ?? new Date().toISOString(),
      durationMs: existing?.durationMs ?? durationMs ?? null,
      total,
      images,
      videos,
    }

    this._upsert(match, entry)
    await this._save()
    return entry.id
  },
})
