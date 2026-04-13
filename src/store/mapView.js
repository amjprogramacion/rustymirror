import { defineStore } from 'pinia'

// Persists the map view (zoom + center) for each metadata panel independently
// so switching between tools restores the exact position the user left.
export const useMapViewStore = defineStore('mapView', {
  state: () => ({
    // BatchEditPanel (MetadataView / metadata tool)
    metadata: null, // { zoom, lat, lon }
    // ImageDetailPanel (DuplicatesView / duplicates tool)
    duplicates: null,    // { zoom, lat, lon }
  }),
})
