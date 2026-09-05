" uf for Vim, through vim-lsp (prabirshrestha/vim-lsp).
"
" Configuration, not a plugin. Source this from your vimrc after vim-lsp is on
" the runtime path.
"
" The awkward part is the working directory, and it is not hidden here.
" `uf lsp` reads `uf.config.js` from the directory the process was started in,
" once, at start-up, and that read is where a project's formatter width, quote
" style and lint levels come from. vim-lsp starts a server in Vim's working
" directory and has no per-server override, and `uf lsp --cwd` is accepted and
" ignored. So either start Vim from the project root, or use the wrapper at the
" bottom of this file, which does the `cd` itself.

if exists('g:loaded_uf_lsp')
  finish
endif
let g:loaded_uf_lsp = 1

" The binary. Point at './node_modules/.bin/uf' for a project's own copy.
if !exists('g:uf_executable')
  let g:uf_executable = 'uf'
endif

" Start the server through a shell that changes into the project root first.
" Off by default because it needs a POSIX shell; on Windows, start Vim from the
" project root instead.
if !exists('g:uf_cd_to_root')
  let g:uf_cd_to_root = 0
endif

" uf's single configuration surface, and therefore the project marker.
let s:config = 'uf.config.js'

function! s:root() abort
  return lsp#utils#find_nearest_parent_file_directory(
        \ lsp#utils#get_buffer_path(), s:config)
endfunction

function! s:cmd() abort
  if !g:uf_cd_to_root
    return [g:uf_executable, 'lsp']
  endif
  let l:root = s:root()
  if empty(l:root)
    return [g:uf_executable, 'lsp']
  endif
  return ['/bin/sh', '-c', 'cd ' . shellescape(l:root) . ' && exec '
        \ . shellescape(g:uf_executable) . ' lsp']
endfunction

function! s:register() abort
  call lsp#register_server({
        \ 'name': 'uf',
        \ 'cmd': {server_info->s:cmd()},
        \ 'root_uri': {server_info->lsp#utils#path_to_uri(s:root())},
        \ 'allowlist': ['javascript', 'javascriptreact', 'javascript.jsx'],
        \ })
endfunction

augroup uf_lsp
  autocmd!
  " Only where there is a uf project to serve: `root_uri` returning an empty
  " URI is what stops vim-lsp starting the server in a project that is not one.
  autocmd User lsp_setup call s:register()
augroup END

" Format on save, off by default. Add to your vimrc if you want it:
"
"   autocmd BufWritePre *.js,*.jsx,*.mjs,*.cjs call execute('LspDocumentFormatSync')
