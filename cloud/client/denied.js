document.getElementById('out').addEventListener('click', async () => {
  if (window.wpOffline && !(await window.wpOffline.beforeLogout())) return; // 6단계: 못 올린 고친 것 확인 · 보관 지우기
  await fetch('/auth/logout', { method: 'POST', headers: { 'x-worshipcue': '1' } }).catch(() => {});
  location.replace('/login?next=' + encodeURIComponent(location.pathname + location.search));
});
