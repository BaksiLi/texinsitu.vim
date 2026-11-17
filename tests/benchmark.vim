" Isolated screen benchmark; launched by benchmark.py, never in a user's Vim.
set nocompatible encoding=utf-8 nomore
execute 'set runtimepath^='.fnameescape($TEXINSITU_BENCH_REPO)
filetype plugin indent on
syntax enable
runtime plugin/texinsitu.vim
set lines=45 columns=100 laststatus=0 scrolloff=0
let &showbreak = '↳ '

function! s:Wait() abort
  let l:start = reltime()
  while reltimefloat(reltime(l:start)) < 10
    if get(b:texinsitu, 'parsed', -1) == b:changedtick
          \ && !has_key(b:texinsitu, 'request') && has_key(b:texinsitu, 'formulas')
      return
    endif
    sleep 1m
  endwhile
  throw 'renderer did not finish'
endfunction

function! s:Median(values) abort
  return sort(copy(a:values), 'f')[len(a:values) / 2]
endfunction

function! s:Move(lnum, col) abort
  let l:start = reltime()
  call cursor(a:lnum, a:col)
  doautocmd CursorMoved
  redraw!
  return reltimefloat(reltime(l:start)) * 1000
endfunction

function! s:Run() abort
  try
    let l:lines = readfile($TEXINSITU_BENCH_SOURCE)
    let l:results = {}
    for l:enabled in [0, 1]
      enew!
      let b:texinsitu_enabled = l:enabled
      call setline(1, l:lines)
      call cursor(1, 1)
      let l:start = reltime()
      setfiletype tex
      setlocal nonumber norelativenumber signcolumn=no foldcolumn=0 nolist
      setlocal wrap linebreak breakindent nofoldenable
      if l:enabled | call s:Wait() | endif
      redraw!
      let l:load = reltimefloat(reltime(l:start)) * 1000
      let l:plain = []
      for l:i in range(60)
        call add(l:plain, s:Move(2 + l:i % 30, 1))
      endfor
      let l:formula = []
      for l:i in range(60)
        call add(l:formula, s:Move(2, l:i % 2 ? 1 : 10))
      endfor
      let l:resize = []
      for l:width in [40, 75, 100, 40, 75, 100]
        let l:start = reltime()
        execute 'set columns='.l:width
        doautocmd VimResized
        redraw!
        call add(l:resize, reltimefloat(reltime(l:start)) * 1000)
      endfor
      call s:Move(2, 1)
      let l:typing = []
      for l:i in range(60)
        let l:start = reltime()
        call setline(2, (l:i % 2 ? 'Edit ' : '').l:lines[1])
        call listener_flush()
        redraw!
        call add(l:typing, reltimefloat(reltime(l:start)) * 1000)
      endfor
      let l:results[l:enabled ? 'rendered' : 'source'] = {
            \ 'load_ms': l:load, 'prose_move_ms': s:Median(l:plain),
            \ 'formula_move_ms': s:Median(l:formula), 'resize_ms': s:Median(l:resize),
            \ 'line_edit_ms': s:Median(l:typing)}
    endfor
    call writefile([json_encode(l:results)], $TEXINSITU_BENCH_RESULT)
  catch
    call writefile([json_encode({'error': v:exception, 'at': v:throwpoint})],
          \ $TEXINSITU_BENCH_RESULT)
    cquit
  endtry
  qall!
endfunction

autocmd VimEnter * ++nested call <SID>Run()
