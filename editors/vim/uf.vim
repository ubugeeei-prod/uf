" uf for Vim, through vim-lsp (prabirshrestha/vim-lsp).
"
" Configuration, not a plugin. Source this from your vimrc after vim-lsp is on
" the runtime path.
"
" The part that matters is which project the server reads. `uf lsp` reads
" `uf.config.js` once, at start-up — from the directory `--cwd` names, or else
" from its own working directory — and that read is where a project's formatter
" width, quote style and lint levels come from. vim-lsp starts every server in
" Vim's working directory and has no per-server override, so the command below
" names the project with `--cwd` instead.

if exists('g:loaded_uf_lsp')
  finish
endif
let g:loaded_uf_lsp = 1

" The binary. Point at './node_modules/.bin/uf' for a project's own copy.
if !exists('g:uf_executable')
  let g:uf_executable = 'uf'
endif

" uf's single configuration surface, and therefore the project marker.
let s:config = 'uf.config.js'

function! s:root() abort
  return lsp#utils#find_nearest_parent_file_directory(
        \ lsp#utils#get_buffer_path(), s:config)
endfunction

" The project root as an argument rather than a `cd`: no shell to find or quote
" for, so it is the same command on every platform Vim runs on.
function! s:cmd() abort
  let l:root = s:root()
  if empty(l:root)
    return [g:uf_executable, 'lsp']
  endif
  return [g:uf_executable, 'lsp', '--cwd', l:root]
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
