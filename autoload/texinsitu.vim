" texinsitu.vim: TeX math rendered at its source position; the buffer stays
" source. Vim reserves a wrapped line's height from its source, concealed or
" not, and cannot add screen rows inside a wrapped line. So prose stays real
" text and inline math stays on one row; only standalone equations use a text
" grid, spread over the source lines they replace.

let s:root = expand('<sfile>:p:h:h')
let s:kernel = s:root.'/kernel/target/release/texinsitu'.(has('win32') ? '.exe' : '')

function! texinsitu#enabled() abort
  if !executable(s:kernel) && !exists('s:unbuilt')
    let s:unbuilt = 1
    echomsg 'TeXinsitu: run :TexInsituInstall to install the kernel; see :help texinsitu-install'
  endif
  return get(b:, 'texinsitu_enabled', get(g:, 'texinsitu_enabled', 1))
        \ && has('conceal') && has('patch-9.0.0121') && has('textprop')
        \ && has('job') && has('channel') && has('timers')
        \ && executable(s:kernel)
endfunction

let s:types = {'': 'TexInsituInline', 'sub': 'TexInsituSub',
      \ 'sup': 'TexInsituSup', 'row': 'TexInsituRow'}

function! s:WritingBuffer() abort
  return &l:buftype ==# '' && index(g:texinsitu_filetypes, &l:filetype) >= 0
endfunction

function! texinsitu#leave() abort
  for l:id in get(w:, 'texinsitu_matches', [])
    silent! call matchdelete(l:id)
  endfor
  unlet! w:texinsitu_matches
  if exists('w:texinsitu_options')
    let [&l:conceallevel, &l:concealcursor] = w:texinsitu_options
    unlet w:texinsitu_options
  endif
endfunction

" Match conceal also enables syntax conceal. Load this buffer's syntax with
" its conceal defaults off, so revealed math is source; never leave another
" plugin's globals changed. Explicit user settings still take precedence.
function! texinsitu#syntax() abort
  let l:state = get(b:, 'texinsitu', {})
  if empty(l:state) || empty(&l:syntax) || get(l:state, 'loading_syntax', 0) | return | endif
  let l:defaults = &l:syntax ==# 'tex'
        \ ? {'tex_conceal': '', 'vimtex_syntax_conceal_disable': 1}
        \ : &l:syntax ==# 'markdown' ? {'markdown_syntax_conceal': 0} : {}
  call filter(l:defaults, '!has_key(g:, v:key)')
  if empty(l:defaults) | return | endif
  let [l:state.syntax, l:state.loading_syntax] = [1, 1]
  call extend(g:, l:defaults)
  try
    let &l:syntax = &l:syntax
  finally
    for l:name in keys(l:defaults) | call remove(g:, l:name) | endfor
    unlet l:state.loading_syntax
  endtry
endfunction

function! s:Conceal(positions) abort
  for l:id in get(w:, 'texinsitu_matches', [])
    silent! call matchdelete(l:id)
  endfor
  if !exists('w:texinsitu_options')
    let w:texinsitu_options = [&l:conceallevel, &l:concealcursor]
  endif
  setlocal conceallevel=2 concealcursor=nvic
  let w:texinsitu_matches = []
  for l:offset in range(0, len(a:positions) - 1, 8)
    call add(w:texinsitu_matches, matchaddpos('Conceal',
          \ a:positions[l:offset : l:offset + 7], 100, -1, {'conceal': ''}))
  endfor
endfunction

function! s:Windows(buf) abort
  return filter(getwininfo(), 'v:val.bufnr == a:buf')
endfunction

" One row of [text, kind] segments as virtual text at a column.
function! s:AddRow(buf, f, lnum, col, row, ...) abort
  for [l:text, l:kind] in a:row
    call add(a:f.props, prop_add(a:lnum, a:col, extend({'bufnr': a:buf,
          \ 'type': s:types[empty(l:kind) && a:f.display ? 'row' : l:kind],
          \ 'text': l:text}, get(a:, 1, {}))))
  endfor
endfunction

function! s:RowWidth(row) abort
  return strdisplaywidth(join(map(copy(a:row), 'v:val[0]'), ''))
endfunction

" Keep closing punctuation beside inline math. Vim counts both the hidden
" source and virtual text when wrapping, so reserve room for the whole group.
function! s:InlineTail(f) abort
  return matchstr(strpart(a:f.line, a:f.last[1] - 1),
        \ '^[.,;:!?)\]。，、；：！？）」』】》〉”’]*')
endfunction

function! s:InlineWidth(f) abort
  let l:tail = s:InlineTail(a:f)
  return empty(l:tail) ? 0 : s:RowWidth(a:f.segments)
        \ + strdisplaywidth(strpart(a:f.line, a:f.first[1] - 1,
        \ a:f.last[1] - a:f.first[1]).l:tail)
endfunction

function! s:VirtualCol(win, pos) abort
  return str2nr(trim(win_execute(a:win, 'echo virtcol('.string(a:pos).')')))
endfunction

" Show one formula: inline segments at its column, or its grid over its lines.
function! s:Show(buf, f, width, win) abort
  let a:f.props = []
  if !a:f.display
    let l:needed = a:f.inline_width
    if l:needed && getwinvar(a:win, '&wrap')
      let l:columns = a:width + 2
      let l:start = s:VirtualCol(a:win, a:f.first) - 1
      let l:room = l:columns - l:start % l:columns
      if l:needed > l:room
        call s:AddRow(a:buf, a:f, a:f.first[0], a:f.first[1],
              \ [[repeat(' ', l:room), '']])
      endif
    endif
    call s:AddRow(a:buf, a:f, a:f.first[0], a:f.first[1], a:f.segments)
    if l:needed && getwinvar(a:win, '&wrap')
      " Native columns also include hanging indent and continuation markers.
      " If those still split the group, restore source instead of a stray mark.
      let l:start = s:VirtualCol(a:win, a:f.first) - 1 - s:RowWidth(a:f.segments)
      let l:end = s:VirtualCol(a:win,
            \ [a:f.last[0], a:f.last[1] + strlen(s:InlineTail(a:f)) - 1]) - 1
      if l:start / l:columns != l:end / l:columns
        call s:Hide(a:buf, a:f)
        return 0
      endif
    endif
    return 1
  endif
  " A concealed source line still takes the screen rows its source needs, so
  " a grid row fits only in the room left beside it, and a wrapped line can
  " end the grid but not sit inside it. Of the places where the most rows
  " fit, the grid takes the one nearest the middle of its source lines, and
  " rows that do not fit continue above.
  let l:lines = getbufline(a:buf, a:f.first[0], a:f.last[0])
  let l:columns = a:width + 2
  let [l:room, l:wrapped] = [[], []]
  for l:line in l:lines
    let l:used = strdisplaywidth(l:line)
    let l:rows = max([1, (l:used + l:columns - 1) / l:columns])
    call add(l:room, l:rows * l:columns - l:used)
    call add(l:wrapped, l:rows > 1)
  endfor
  " A label sits right of the last row, as an equation number does: past the
  " widest row, else just past the last; on its own row only if neither fits.
  let l:block = map(copy(a:f.rows), '[["│ ", ""]] + v:val')
  let l:blocks = [l:block]
  if !empty(a:f.label)
    let l:last = [['└ ', '']] + a:f.rows[-1]
    let l:wide = max(map(copy(l:block), 's:RowWidth(v:val)')) - s:RowWidth(l:last)
    let l:blocks = map(uniq([l:wide + 2, 2]), 'l:block[:-2] + [l:last + [[repeat(" ", v:val).a:f.label, ""]]]')
          \ + [l:block + [[['└ '.a:f.label, '']]]]
  endif
  for l:block in l:blocks
    let [l:count, l:end] = s:Place(l:block, l:room, l:wrapped)
    if l:count == len(l:block) | break | endif
  endfor
  let [l:n, l:size] = [len(l:block), len(l:lines)]
  let l:top = l:end - l:count + 1
  let l:texts = repeat([[]], l:size)
  if l:count | let l:texts[l:top : l:end] = l:block[l:n - l:count :] | endif
  for l:row in l:count < l:n ? l:block[: l:n - l:count - 1] : []
    call s:AddRow(a:buf, a:f, a:f.first[0] + (l:count ? l:top : 0), 0,
          \ [[join(map(copy(l:row), 'v:val[0]'), ''), '']],
          \ {'text_align': 'above', 'text_wrap': 'truncate'})
  endfor
  for l:index in range(len(l:lines))
    if !empty(l:texts[l:index])
      call s:AddRow(a:buf, a:f, a:f.first[0] + l:index, 1, l:texts[l:index])
    endif
  endfor
  return 1
endfunction

" The most trailing rows of block that fit on consecutive lines, nearest the
" middle of the lines, as [count, index of the line holding the last row].
function! s:Place(block, room, wrapped) abort
  let [l:n, l:count, l:end, l:miss] = [len(a:block), 0, 0, 0]
  for l:last in range(len(a:room))
    let l:k = 0
    while l:k < l:n && l:k <= l:last && (l:k == 0 || !a:wrapped[l:last - l:k])
          \ && s:RowWidth(a:block[l:n - 1 - l:k]) <= a:room[l:last - l:k]
      let l:k += 1
    endwhile
    " Twice the distance between the grid's middle and the lines' middle.
    let l:off = abs(2 * l:last - l:k + 2 - len(a:room))
    if l:k > l:count || (l:k && l:k == l:count && l:off <= l:miss)
      let [l:count, l:end, l:miss] = [l:k, l:last, l:off]
    endif
  endfor
  return [l:count, l:end]
endfunction

function! s:Hide(buf, f) abort
  " Virtual text has Vim-assigned ids, kept when the formula was shown.
  for l:id in get(a:f, 'props', [])
    call prop_remove({'id': l:id, 'bufnr': a:buf, 'all': 1})
  endfor
  let a:f.props = []
endfunction

function! s:Inside(f, line, col) abort
  if a:f.display
    return a:line >= a:f.first[0] && a:line <= a:f.last[0]
  endif
  return a:line == a:f.first[0] && a:col >= a:f.first[1] && a:col < a:f.last[1]
endfunction

" Formulas being edited in any view show source; all others are rendered.
function! s:Active(buf, state) abort
  let l:active = []
  for l:win in s:Windows(a:buf)
    let [l:line, l:col] = getcurpos(l:win.winid)[1:2]
    let l:visual = l:win.winid == win_getid() && mode() =~# '^[vV\x16]'
    let l:range = l:visual ? sort([line('v'), l:line], 'n') : [l:line, l:line]
    for l:lnum in range(l:range[0], l:range[1])
      for l:f in get(a:state.byline, l:lnum, [])
        " Visual: reveal every formula in the selection; otherwise only the
        " formula the cursor is actually inside, not the whole buffer line.
        if (l:visual || (l:lnum == l:line && s:Inside(l:f, l:line, l:col)))
              \ && index(l:active, l:f.id) < 0
          call add(l:active, l:f.id)
        endif
      endfor
    endfor
  endfor
  return sort(l:active, 'n')
endfunction

function! texinsitu#paint(buf, ...) abort
  let l:state = getbufvar(a:buf, 'texinsitu', {})
  let l:windows = s:Windows(a:buf)
  if !has_key(l:state, 'formulas') || empty(l:windows) | return | endif
  let l:width = min(map(copy(l:windows), 'v:val.width - v:val.textoff')) - 2
  let l:layouts = map(copy(l:windows), '[v:val.width - v:val.textoff,
        \ getwinvar(v:val.winid, "&wrap"), getwinvar(v:val.winid, "&linebreak"),
        \ getwinvar(v:val.winid, "&breakindent"), getwinvar(v:val.winid, "&showbreak"),
        \ getwinvar(v:val.winid, "&breakindentopt"), getwinvar(v:val.winid, "&tabstop")]')
  let l:signature = [s:Active(a:buf, l:state), l:width,
        \ map(copy(l:windows), 'v:val.winid'), l:layouts, &breakat]
  let l:edited = get(a:, 1, 0)
  if l:signature ==# l:state.painted && !l:edited | return | endif
  let l:previous = l:state.painted
  let l:state.painted = l:signature
  let l:shared = len(uniq(sort(map(copy(l:layouts), 'string(v:val)')))) == 1
  let l:resized = l:layouts !=# get(l:state, 'layouts', [])
        \ || &breakat !=# get(l:state, 'breakat', '')
  let l:full = empty(l:previous) || l:resized || l:previous[2] !=# l:signature[2]
  if l:full
    let l:formulas = l:state.formulas
  else
    " A cursor move changes only the entered/left formulas and their inline
    " neighbours: earlier virtual text can change the rest of that row's wrap.
    let l:affected = {}
    for l:f in get(l:state.byline, l:edited, [])
      let l:affected[l:f.id] = l:f
    endfor
    for l:id in uniq(sort(copy(l:previous[0]) + l:signature[0], 'n'))
      let l:f = l:state.byid[l:id]
      for l:neighbour in l:f.display ? [l:f] : l:state.byline[l:f.first[0]]
        let l:affected[l:neighbour.id] = l:neighbour
      endfor
    endfor
    let l:formulas = map(sort(keys(l:affected), 'n'), 'l:affected[v:val]')
  endif
  let l:wanted = l:full ? {} : copy(l:state.shown)
  let l:before = {}
  let l:reflow = {}
  for l:f in l:formulas
    let l:before[l:f.id] = has_key(l:state.shown, l:f.id)
    if has_key(l:wanted, l:f.id) | call remove(l:wanted, l:f.id) | endif
    let l:tail = l:f.display ? 0 : l:f.inline_width
    " Shared text properties cannot place different padding in unequal views.
    " When the group cannot fit, leave this formula as editable source.
    let l:fits = l:f.display ? l:f.width <= l:width
          \ : !l:tail || (l:shared && (!l:layouts[0][1] || l:tail <= l:width))
    if index(l:signature[0], l:f.id) < 0 && l:fits
      let l:wanted[l:f.id] = l:f
    endif
    if !l:f.display && (l:resized
          \ || has_key(l:state.shown, l:f.id) != has_key(l:wanted, l:f.id))
      let l:reflow[l:f.first[0]] = 1
    endif
  endfor
  let l:state.width = l:width
  let l:state.layouts = l:layouts
  let l:state.breakat = &breakat
  if l:resized
    " Removing each property separately repeatedly scans a long buffer.
    call prop_remove({'types': values(s:types), 'bufnr': a:buf, 'all': 1})
    for l:f in values(l:state.shown) | let l:f.props = [] | endfor
    let l:state.shown = {}
  else
    for l:f in l:formulas
      if has_key(l:state.shown, l:f.id) && (!has_key(l:wanted, l:f.id)
            \ || (!l:f.display && has_key(l:reflow, l:f.first[0])))
        call s:Hide(a:buf, l:f)
        call remove(l:state.shown, l:f.id)
      endif
    endfor
  endif
  " Earlier virtual text affects the available room of later formulas.
  for l:f in l:formulas
    let l:id = l:f.id
    if !has_key(l:wanted, l:id) | continue | endif
    if !has_key(l:state.shown, l:id) && !s:Show(a:buf, l:f, l:width, l:windows[0].winid)
      call remove(l:wanted, l:id)
    endif
  endfor
  let l:state.shown = l:wanted
  if empty(l:previous) || l:previous[2] !=# l:signature[2]
        \ || !empty(filter(copy(l:formulas), 'l:before[v:val.id] != has_key(l:wanted, v:val.id)'))
    call s:Refresh(l:state)
  endif
endfunction

function! s:Refresh(state) abort
  let l:positions = []
  for l:f in values(a:state.shown)
    call extend(l:positions, l:f.spans)
  endfor
  for l:winid in a:state.painted[2]
    call win_execute(l:winid, 'call '.expand('<SID>').'Conceal('.string(l:positions).')')
  endfor
endfunction

function! s:Index(state) abort
  let a:state.byline = {}
  let a:state.byid = {}
  for l:f in a:state.formulas
    let a:state.byid[l:f.id] = l:f
    for l:lnum in range(l:f.first[0], l:f.last[0])
      let a:state.byline[l:lnum] = get(a:state.byline, l:lnum, []) + [l:f]
    endfor
  endfor
endfunction

function! s:Install(buf, formulas) abort
  let l:state = getbufvar(a:buf, 'texinsitu')
  call prop_remove({'types': values(s:types), 'bufnr': a:buf, 'all': 1})
  for l:index in range(len(a:formulas))
    let l:f = a:formulas[l:index]
    let l:f.id = l:index + 1
    let l:f.line = getbufline(a:buf, l:f.first[0])[0]
    if !l:f.display | let l:f.inline_width = s:InlineWidth(l:f) | endif
  endfor
  call extend(l:state, {'formulas': a:formulas, 'shown': {}, 'painted': []})
  call s:Index(l:state)
  call texinsitu#paint(a:buf)
endfunction

" Edits keep untouched formulas: lines and columns shift, touched ones go.
" Typing within a line only visits the formulas on that line, and conceal
" positions are rebuilt only when one of them moved or went.
function! s:Edited(buf, start, end, added, changes) abort
  let l:state = getbufvar(a:buf, 'texinsitu', {})
  if !has_key(l:state, 'formulas') | return | endif
  if a:added
    let l:candidates = filter(copy(l:state.formulas), 'v:val.last[0] >= a:start')
  else
    let l:candidates = []
    for l:lnum in range(a:start, a:end - 1)
      call extend(l:candidates, filter(copy(get(l:state.byline, l:lnum, [])),
            \ 'index(l:candidates, v:val) < 0'))
    endfor
  endif
  if empty(l:candidates) | return | endif
  let l:line = a:added == 0 && a:end == a:start + 1 ? getbufline(a:buf, a:start)[0] : v:null
  let l:diff = {}
  let l:moved = a:added != 0
  let l:dropped = 0
  for l:f in l:candidates
    if l:f.first[0] >= a:end
      for l:position in [l:f.first, l:f.last] + l:f.spans
        let l:position[0] += a:added
      endfor
      continue
    endif
    if l:line isnot v:null && !l:f.display
      if !has_key(l:diff, l:f.line) | let l:diff[l:f.line] = s:Diff(l:f.line, l:line) | endif
      let [l:prefix, l:end, l:delta] = l:diff[l:f.line]
      " Untouched formulas lie wholly before or after the changed bytes.
      if l:f.last[1] - 1 <= l:prefix || l:f.first[1] - 1 >= l:end
        let l:f.line = l:line
        if l:delta && l:f.first[1] - 1 >= l:end
          let l:f.first[1] += l:delta
          let l:f.last[1] += l:delta
          let l:f.spans[0][1] += l:delta
          let l:moved = 1
        endif
        let l:f.inline_width = s:InlineWidth(l:f)
        continue
      endif
    endif
    call s:Hide(a:buf, l:f)
    call remove(l:state.formulas, index(l:state.formulas, l:f))
    if has_key(l:state.shown, l:f.id) | call remove(l:state.shown, l:f.id) | endif
    let l:dropped = 1
  endfor
  if l:dropped || a:added | call s:Index(l:state) | endif
  if l:line isnot v:null
    " An edit before a formula, or a removed rendering, changes its padding.
    for l:f in get(l:state.byline, a:start, [])
      if !l:f.display && has_key(l:state.shown, l:f.id)
        call s:Hide(a:buf, l:f)
        call remove(l:state.shown, l:f.id)
      endif
    endfor
  endif
  if l:dropped
    " A deleted formula also retires its cursor-reveal state.
    let l:state.painted = []
  endif
  if l:line isnot v:null || l:dropped
    call texinsitu#paint(a:buf, l:line isnot v:null ? a:start : 0)
  elseif l:moved && !empty(l:state.painted)
    call s:Refresh(l:state)
  endif
endfunction

" [common prefix bytes, end of the changed bytes in old, length change].
function! s:Diff(old, new) abort
  let l:limit = min([strlen(a:old), strlen(a:new)])
  let [l:low, l:high] = [0, l:limit]
  while l:low < l:high
    let l:mid = (l:low + l:high + 1) / 2
    if strpart(a:old, 0, l:mid) ==# strpart(a:new, 0, l:mid)
      let l:low = l:mid
    else
      let l:high = l:mid - 1
    endif
  endwhile
  let l:prefix = l:low
  let [l:low, l:high] = [0, l:limit - l:prefix]
  while l:low < l:high
    let l:mid = (l:low + l:high + 1) / 2
    if strpart(a:old, strlen(a:old) - l:mid) ==# strpart(a:new, strlen(a:new) - l:mid)
      let l:low = l:mid
    else
      let l:high = l:mid - 1
    endif
  endwhile
  return [l:prefix, strlen(a:old) - l:low, strlen(a:new) - strlen(a:old)]
endfunction

" One renderer per Vim keeps formula layouts between parses; each buffer has
" at most one request in flight, and its reply schedules the next if needed.
" A whole buffer and its reply each outgrow a pipe, so Vim queues what the
" renderer has not read yet instead of waiting while the reply waits for it.
function! s:Server() abort
  if !exists('s:job') || job_status(s:job) !=# 'run'
    let s:job = job_start([s:kernel], {
          \ 'in_io': 'pipe', 'noblock': 1, 'out_mode': 'nl', 'out_cb': function('s:Rendered'),
          \ 'err_cb': function('s:Error'), 'exit_cb': function('s:Exited'),
          \ 'stoponexit': 'term'})
  endif
  return job_getchannel(s:job)
endfunction

function! s:Rendered(channel, output) abort
  let l:reply = json_decode(a:output)
  if !has_key(s:requests, l:reply.id) | return | endif
  let l:buf = remove(s:requests, l:reply.id)
  let l:state = getbufvar(l:buf, 'texinsitu', {})
  if get(l:state, 'request') != l:reply.id | return | endif
  unlet l:state.request
  if getbufvar(l:buf, 'changedtick') == l:state.parsed
    call s:Install(l:buf, l:reply.formulas)
  else
    call texinsitu#schedule(l:buf)
  endif
endfunction

" Requests in flight are forgotten; the next edit or window entry parses again.
function! s:Exited(job, status) abort
  if !exists('s:job') || a:job != s:job | return | endif
  for [l:id, l:buf] in items(s:requests)
    let l:state = getbufvar(l:buf, 'texinsitu', {})
    if get(l:state, 'request') == l:id
      unlet l:state.request l:state.parsed
    endif
  endfor
  let s:requests = {}
endfunction

function! s:Parse(buf, ...) abort
  let l:state = getbufvar(a:buf, 'texinsitu', {})
  if empty(l:state) || !bufloaded(a:buf) || has_key(l:state, 'request') | return | endif
  let s:serial += 1
  let s:requests[s:serial] = a:buf
  let l:state.request = s:serial
  let l:state.parsed = getbufvar(a:buf, 'changedtick')
  let l:state.context = s:Context(a:buf)
  call ch_sendraw(s:Server(), json_encode({'id': s:serial,
        \ 'lines': getbufline(a:buf, 1, '$'),
        \ 'filetype': getbufvar(a:buf, '&filetype'),
        \ 'context': l:state.context,
        \ 'cells': s:Cells()})."\n")
endfunction

" Cell widths as Vim counts them, so the kernel's grids line up on screen.
function! s:Cells() abort
  return {'wide': &ambiwidth ==# 'double' ? v:true : v:false,
        \ 'overrides': exists('*getcellwidths') ? getcellwidths() : []}
endfunction

function! s:Context(buf) abort
  let l:main = get(getbufvar(a:buf, 'vimtex', {}), 'tex', '')
  if empty(l:main) || fnamemodify(bufname(a:buf), ':p') ==# l:main
    return ''
  endif
  return join(bufloaded(l:main) ? getbufline(l:main, 1, '$')
        \ : filereadable(l:main) ? readfile(l:main) : [], "\n")
endfunction

function! s:Error(channel, message) abort
  " Keep diagnostics in :messages without popups while someone is typing.
  echomsg 'TeXinsitu: '.a:message
endfunction

" Parsing waits until typing pauses outside Insert mode.
function! texinsitu#schedule(buf) abort
  let l:state = getbufvar(a:buf, 'texinsitu', {})
  if empty(l:state) || get(l:state, 'parsed', -1) == getbufvar(a:buf, 'changedtick')
        \ || (a:buf == bufnr('%') && mode() =~# '^[iR]')
    return
  endif
  call timer_stop(get(l:state, 'timer', -1))
  let l:state.timer = timer_start(180, function('s:Parse', [a:buf]))
endfunction


" 'ambiwidth' and setcellwidths() change every width: lay everything out again.
function! texinsitu#relayout() abort
  for l:info in getbufinfo({'bufloaded': 1})
    let l:state = get(l:info.variables, 'texinsitu', {})
    if !empty(l:state)
      let l:state.parsed = -1
      call texinsitu#schedule(l:info.bufnr)
    endif
  endfor
endfunction

function! texinsitu#clear(buf) abort
  let l:state = getbufvar(a:buf, 'texinsitu', {})
  if empty(l:state) | return | endif
  call timer_stop(get(l:state, 'timer', -1))
  call listener_remove(l:state.listener)
  let l:types = filter(prop_type_list(), 'v:val =~# "^TexInsitu"')
  if !empty(l:types)
    call prop_remove({'types': l:types, 'bufnr': a:buf, 'all': 1})
  endif
  let l:windows = s:Windows(a:buf)
  call setbufvar(a:buf, 'texinsitu', {})
  for l:win in l:windows
    call win_execute(l:win.winid, 'call texinsitu#leave()')
  endfor
  if get(l:state, 'syntax', 0) && !empty(l:windows)
    call win_execute(l:windows[0].winid, 'let &l:syntax = &l:syntax')
  endif
endfunction

function! texinsitu#enter() abort
  if !s:WritingBuffer() || !texinsitu#enabled() | return | endif
  for l:type in values(s:types)
    if empty(prop_type_get(l:type))
      call prop_type_add(l:type, {'combine': 0,
            \ 'highlight': l:type =~# 'Sub\|Sup' ? l:type : 'TexInsitu'})
    endif
  endfor
  let l:state = get(b:, 'texinsitu', {})
  if !empty(l:state) && l:state.context !=# s:Context(bufnr('%'))
    call texinsitu#clear(bufnr('%'))
    let l:state = {}
  endif
  if empty(l:state)
    let b:texinsitu = {'context': '',
          \ 'listener': listener_add(function('s:Edited'))}
    call texinsitu#syntax()
    call s:Parse(bufnr('%'))
  endif
  " The window owns reveal from the start, before the first parse returns.
  if !exists('w:texinsitu_matches') | call s:Conceal([]) | endif
  call texinsitu#paint(bufnr('%'))
endfunction

function! texinsitu#toggle() abort
  if !s:WritingBuffer() | return 0 | endif
  let b:texinsitu_enabled = !texinsitu#enabled()
  call texinsitu#clear(bufnr('%'))
  call texinsitu#enter()
  return b:texinsitu_enabled
endfunction

let s:requests = {}
let s:serial = get(s:, 'serial', 0)
if !exists('s:loaded')
  let s:loaded = 1
  finish
endif

" Sourcing again replaces presentation, pending callbacks and the renderer,
" never buffer contents.
if exists('s:job') | call job_stop(s:job) | endif
unlet! s:job
for s:buffer in getbufinfo({'bufloaded': 1})
  call texinsitu#clear(s:buffer.bufnr)
endfor
for s:window in getwininfo()
  call win_execute(s:window.winid, 'call texinsitu#enter()')
endfor
unlet! s:buffer s:window
