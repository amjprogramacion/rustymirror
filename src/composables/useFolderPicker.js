import { open } from '@tauri-apps/plugin-dialog'

/**
 * Shared helper for the directory-picker flow used in every sidebar.
 *
 *   const { pickDirectory } = useFolderPicker()
 *   pickDirectory(path => store.addFolder(path))
 */
export function useFolderPicker() {
  async function pickDirectory(onPicked) {
    const path = await open({ directory: true, multiple: false })
    if (path) onPicked(path)
  }
  return { pickDirectory }
}
