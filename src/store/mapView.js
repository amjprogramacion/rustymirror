import { defineStore } from 'pinia'

// Persists the map view (zoom + center) for each metadata panel independently
// so switching between tools restores the exact position the user left.
export const useMapViewStore = defineStore('mapView', {
  state: () => ({
    // MetadataBottomPanel (MetadataManager / metadata tool)
    metadataManager: null, // { zoom, lat, lon }
    // MetadataPanel (ResultsArea / duplicates tool)
    resultsPanel: null,    // { zoom, lat, lon }
  }),
})
