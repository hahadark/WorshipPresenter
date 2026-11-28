# 웹판 작업용: 줄 끝(CRLF/LF)을 그대로 두고 글 바꾸기
import sys
def edit(p, pairs):
    b=open(p,'rb').read()
    crlf=b'\r\n' in b
    s=b.decode('utf-8').replace('\r\n','\n')
    for old,new,cnt in pairs:
        assert s.count(old)>=1, (p, old)
        if cnt: assert s.count(old)==cnt,(p,old,s.count(old))
        s=s.replace(old,new)
    if crlf: s=s.replace('\n','\r\n')
    open(p,'wb').write(s.encode('utf-8'))
W='#[cfg(target_arch = "wasm32")]\n'
N='#[cfg(not(target_arch = "wasm32"))]\n'
