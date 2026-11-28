// 드라이브 폴더: '워십프리젠터/<교회 이름>/<종류>/' 를 처음 쓸 때 만든다. 폴더 id 는 D1 drive_accounts 에 (작은 것만).
// 두 요청이 동시에 같은 폴더를 만들면 먼저 적은 것을 쓰고 늦은 쪽이 만든 폴더는 휴지통으로 (json_set 조건부 쓰기).
import { createFolder, drive, accountOf } from './google.mjs';
import { FILE_FOLDERS, KIND_FOLDERS, ROOT_FOLDER, cleanName } from './format.mjs';

const memo = new Map(); // 교회 → { at, root, church, folders }
const MEMO_MS = 5 * 60_000;
export function forgetFolders(church) { memo.delete(church); }

async function load(env, church) {
  const hit = memo.get(church);
  if (hit && Date.now() - hit.at < MEMO_MS) return hit;
  const a = await accountOf(env, church);
  if (!a) return null;
  let folders = {};
  try { folders = JSON.parse(a.folders || '{}'); } catch { /* 처음부터 */ }
  const v = { at: Date.now(), root: a.root_id, church: a.church_folder_id, folders };
  memo.set(church, v);
  return v;
}

/** '워십프리젠터/<교회 이름>' 폴더 id (없으면 만든다) */
export async function churchFolder(env, church) {
  let f = await load(env, church);
  if (f?.church) return f.church;
  const c = await env.DB.prepare('SELECT name FROM churches WHERE id = ?1').bind(church).first();
  let root = f?.root;
  if (!root) {
    root = await createFolder(env, church, ROOT_FOLDER, null);
    const r = await env.DB.prepare('UPDATE drive_accounts SET root_id = ?2 WHERE church_id = ?1 AND root_id IS NULL').bind(church, root).run();
    if (!r.meta.changes) { await trash(env, church, root); forgetFolders(church); f = await load(env, church); root = f.root; }
  }
  const id = await createFolder(env, church, cleanName(c?.name || church, 60) || church, root);
  const r = await env.DB.prepare('UPDATE drive_accounts SET church_folder_id = ?2 WHERE church_id = ?1 AND church_folder_id IS NULL').bind(church, id).run();
  forgetFolders(church);
  if (!r.meta.changes) { await trash(env, church, id); return (await load(env, church)).church; }
  return id;
}

/** 종류 폴더 id. key: 'kind:cue' · 'bible' · 'files' · 'files:image' … */
export async function folderFor(env, church, key) {
  const f = await load(env, church);
  if (f?.folders[key]) return f.folders[key];
  let parent, name;
  if (key.startsWith('kind:')) { parent = await churchFolder(env, church); name = KIND_FOLDERS[key.slice(5)] || key.slice(5); }
  else if (key === 'bible') { parent = await churchFolder(env, church); name = '성경'; }
  else if (key === 'files') { parent = await churchFolder(env, church); name = '파일'; }
  else if (key.startsWith('files:')) { parent = await folderFor(env, church, 'files'); name = FILE_FOLDERS[key.slice(6)] || key.slice(6); }
  else throw new Error(`모르는 폴더: ${key}`);
  const id = await createFolder(env, church, name, parent);
  const path = `$."${key.replace(/"/g, '')}"`;
  const r = await env.DB.prepare(`UPDATE drive_accounts SET folders = json_set(folders, ?2, ?3) WHERE church_id = ?1 AND json_extract(folders, ?2) IS NULL`).bind(church, path, id).run();
  forgetFolders(church);
  if (!r.meta.changes) { await trash(env, church, id); return (await load(env, church)).folders[key]; }
  return id;
}

/** 휴지통으로 (사람이 드라이브에서 되살릴 수 있게 바로 지우지 않는다) */
export async function trash(env, church, fileId) {
  await drive(env, church, `/files/${encodeURIComponent(fileId)}?fields=id`, { method: 'PATCH', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ trashed: true }) }).catch(() => null);
}
