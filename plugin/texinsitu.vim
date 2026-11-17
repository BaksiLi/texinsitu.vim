" texinsitu.vim: TeX math rendered in place in TeX, Markdown and Cogitatum.
if exists('g:loaded_texinsitu')
  finish
endif
let g:loaded_texinsitu = 1

if !has('patch-9.0.0121') || !has('textprop') || !has('conceal')
      \ || !has('job') || !has('channel') || !has('timers')
  finish
endif

let g:texinsitu_filetypes = get(g:, 'texinsitu_filetypes', ['tex', 'markdown', 'cogitatum'])

command! -bar TexInsituToggle call texinsitu#toggle()

function! s:Highlights() abort
  highlight default link TexInsitu Special
  " Scripts keep the formula colour; underline reads as below the baseline.
  " Some terminals draw no italic, so there a superscript is also bold.
  let l:id = synIDtrans(hlID('TexInsitu'))
  let l:colours = ''
  for [l:key, l:mode, l:what] in [['guifg', 'gui', 'fg#'], ['ctermfg', 'cterm', 'fg']]
    let l:value = synIDattr(l:id, l:what, l:mode)
    if !empty(l:value) | let l:colours .= ' '.l:key.'='.l:value | endif
  endfor
  execute 'highlight default TexInsituSub gui=italic,underline cterm=italic,underline'.l:colours
  execute 'highlight default TexInsituSup gui=italic cterm=italic,bold'.l:colours
endfunction

function! s:Wanted() abort
  return index(g:texinsitu_filetypes, &l:filetype) >= 0 || exists('b:texinsitu')
endfunction

function! s:Reset() abort
  call texinsitu#clear(str2nr(expand('<abuf>')))
  call texinsitu#enter()
endfunction

call s:Highlights()

augroup texinsitu
  autocmd!
  " Reading or reloading a file sets its filetype again, so FileType covers both.
  autocmd FileType * if s:Wanted() | call s:Reset() | endif
  " The syntax reload needs nested Syntax events; its function guards reentry.
  autocmd Syntax * ++nested if !empty(get(b:, 'texinsitu', {})) | call texinsitu#syntax() | endif
  autocmd BufWinEnter,WinEnter * if s:Wanted() | call texinsitu#enter() | endif
  autocmd BufWinLeave * if exists('w:texinsitu_matches') | call texinsitu#leave() | endif
  autocmd CursorMoved,CursorMovedI,ModeChanged,VimResized,WinNew,WinClosed *
        \ if exists('b:texinsitu') | call texinsitu#paint(bufnr('%')) | endif
  if exists('##WinResized')
    autocmd WinResized * if exists('b:texinsitu') | call texinsitu#paint(bufnr('%')) | endif
  endif
  autocmd OptionSet wrap,linebreak,breakindent,breakindentopt,showbreak,breakat,tabstop,number,relativenumber,numberwidth,signcolumn,foldcolumn
        \ if exists('b:texinsitu') | call texinsitu#paint(bufnr('%')) | endif
  autocmd OptionSet ambiwidth call texinsitu#relayout()
  autocmd TextChanged,InsertLeave * if exists('b:texinsitu') | call texinsitu#schedule(bufnr('%')) | endif
  autocmd BufUnload * if !empty(getbufvar(str2nr(expand('<abuf>')), 'texinsitu'))
        \ | call texinsitu#clear(str2nr(expand('<abuf>'))) | endif
  autocmd ColorScheme * call s:Highlights()
augroup END
