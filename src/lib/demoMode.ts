// Demo mode swaps the real backend and Tauri commands for canned data, so it
// exists only in development builds: the e2e suite and local demos run the dev
// server. A production build ignores ?demo= and any stored flag, so neither a
// crafted URL nor a leftover localStorage key can put a real user in it.
export const isDemoMode = (): boolean => {
  if (!import.meta.env.DEV) return false

  const demoParam = new URLSearchParams(window.location.search).get('demo')
  if (demoParam !== null) {
    const enabled = demoParam === 'true' || demoParam === '1'
    localStorage.setItem('demoMode', enabled.toString())
    return enabled
  }

  return localStorage.getItem('demoMode') === 'true'
}
