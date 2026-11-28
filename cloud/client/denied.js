document.getElementById('out').addEventListener('click', async () => {
  await fetch('/auth/logout', { method: 'POST', headers: { 'x-worshipcue': '1' } }).catch(() => {});
  location.replace('/login?next=' + encodeURIComponent(location.pathname + location.search));
});
