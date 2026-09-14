/* Follow the system until the user chooses a theme. */
(() => {
  const root = document.documentElement;
  const system = matchMedia('(prefers-color-scheme: dark)');
  let preference = null;
  try { preference = localStorage.getItem('mirmir.theme'); } catch {}
  if (!['light', 'dark'].includes(preference)) preference = null;
  const syncButtons = () => {
    const dark = root.dataset.theme === 'dark';
    document.querySelectorAll('[data-appearance]').forEach(button => {
      button.setAttribute('aria-label', dark ? 'Switch to light theme' : 'Switch to dark theme');
      button.setAttribute('title', dark ? 'Switch to light theme' : 'Switch to dark theme');
    });
  };
  const apply = () => {
    root.dataset.theme = preference || (system.matches ? 'dark' : 'light');
    syncButtons();
  };
  apply();
  system.addEventListener('change', apply);
  document.addEventListener('click', event => {
    if (!event.target.closest('[data-appearance]')) return;
    preference = root.dataset.theme === 'dark' ? 'light' : 'dark';
    try { localStorage.setItem('mirmir.theme', preference); } catch {}
    apply();
  });
  new MutationObserver(syncButtons).observe(root, {childList: true, subtree: true});
})();
