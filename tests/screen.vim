" Real screen tests, using generic equivalents of reported manuscript formulas.
" Run with: vim -Nu tests/screen.vim -i NONE -n
set nocompatible encoding=utf-8
let s:repo = fnamemodify(resolve(expand('<sfile>:p')), ':h:h')
let s:root = tempname()
let s:result = $TEXINSITU_RESULT
execute 'set runtimepath^='.fnameescape(s:repo)
filetype plugin indent on
syntax enable
" A plugin must keep highlights already chosen in the user's vimrc.
highlight TexInsituSub cterm=reverse gui=bold guifg=#13579b
highlight TexInsituSup cterm=underline gui=underline guifg=#2468ac
" Wrapped rows are found by their marker.
let &showbreak = '↳ '

function! s:Wait() abort
  let l:start = reltime()
  while reltimefloat(reltime(l:start)) < 5
    sleep 20m
    let l:state = get(b:, 'texinsitu', {})
    if get(l:state, 'parsed', -1) == b:changedtick && !has_key(l:state, 'request')
          \ && has_key(l:state, 'formulas')
      sleep 20m
      redraw!
      return
    endif
  endwhile
  call assert_report('formula parser did not finish')
endfunction

function! s:Move(line, col) abort
  call cursor(a:line, a:col)
  doautocmd CursorMoved
  redraw!
endfunction

function! s:View() abort
  redraw!
  let l:win = getwininfo(win_getid())[0]
  let l:rows = []
  for l:y in range(l:win.winrow, l:win.winrow + l:win.height - 1)
    call add(l:rows, trim(join(map(range(l:win.wincol, l:win.wincol + l:win.width - 1),
          \ 'screenstring(l:y, v:val)'), ''), '', 2))
  endfor
  return l:rows
endfunction

function! s:Buffer(lines, type) abort
  enew!
  call setline(1, a:lines)
  execute 'setfiletype '.a:type
  setlocal nonumber norelativenumber signcolumn=no foldcolumn=0 nolist wrap
  setlocal nofoldenable
  call s:Move(len(a:lines), 1)
  call s:Wait()
endfunction

function! s:Block(rows) abort
  let l:view = s:View()
  let l:start = index(l:view, a:rows[0])
  call assert_true(l:start >= 0, 'grid missing: '.string(a:rows).' in '.string(l:view))
  if l:start >= 0
    call assert_equal(a:rows, l:view[l:start : l:start + len(a:rows) - 1],
          \ 'grid rows stay adjacent')
  endif
endfunction

function! s:Properties() abort
  return prop_list(1, {'types': ['TexInsituInline', 'TexInsituSub',
        \ 'TexInsituSup', 'TexInsituRow'], 'end_lnum': -1})
endfunction

function! s:Editing() abort
  let l:view = join(s:View(), "\n")
  call assert_match('before \$z_a\^2\$ and ν after', l:view,
        \ 'the entered formula shows source in '.mode())
  call assert_notmatch('\\nu', l:view, 'the other formula on the same line stays rendered')
endfunction

function! s:Run() abort
  if !texinsitu#enabled()
    call writefile(['SKIP: texinsitu needs Vim 9.0.0121+ and a built kernel'],
          \ s:result)
    qall!
  endif
  try
    set nomore lines=45 columns=100 laststatus=0 scrolloff=0
    call assert_equal('1', synIDattr(hlID('TexInsituSub'), 'reverse', 'cterm'))
    call assert_equal('#13579b', synIDattr(hlID('TexInsituSub'), 'fg#', 'gui'))
    doautocmd ColorScheme
    call assert_equal('1', synIDattr(hlID('TexInsituSub'), 'reverse', 'cterm'))
    call assert_equal('1', synIDattr(hlID('TexInsituSup'), 'underline', 'cterm'))
    call assert_equal('#2468ac', synIDattr(hlID('TexInsituSup'), 'fg#', 'gui'))
    call assert_false(exists('g:tex_conceal'))
    call assert_false(exists('g:vimtex_syntax_conceal_disable'))
    call assert_false(exists('g:markdown_syntax_conceal'))
    call s:Buffer(['before $z_a^2$ and $\nu$ after', 'Next $Q^\*$ end.', 'after'], 'tex')
    let l:source = getline(1, '$')
    let l:tick = b:changedtick
    let l:modified = &modified
    let l:register = getreginfo('"')
    let l:search = @/
    call assert_true(texinsitu#enabled())
    call s:Block(['before za² and ν after', 'Next Q* end.', 'after'])
    call assert_equal(['TexInsituInline', 'TexInsituSup'],
          \ sort(map(filter(s:Properties(), 'v:val.lnum == 2'), 'v:val.type')),
          \ 'scripts are styled text on the same row')
    let l:other = filter(s:Properties(), 'v:val.lnum == 2')
    let l:parsed = b:texinsitu.parsed
    call s:Move(1, 10)
    call s:Editing()
    call assert_equal(l:other, filter(s:Properties(), 'v:val.lnum == 2'),
          \ 'entering a formula preserves properties on other lines')
    call feedkeys("\<Cmd>call cursor(1, 10)\<CR>i\<Cmd>redraw!\<Bar>call ".expand('<SID>')."Editing()\<CR>\<Esc>", 'xt')
    call s:Move(3, 1)
    call s:Block(['before za² and ν after', 'Next Q* end.'])
    call assert_equal(l:other, filter(s:Properties(), 'v:val.lnum == 2'),
          \ 'leaving a formula preserves properties on other lines')
    call assert_equal(l:parsed, b:texinsitu.parsed, 'movement does not parse again')
    call assert_false(has_key(b:texinsitu, 'request'), 'movement sends no request')
    call assert_equal(l:source, getline(1, '$'))
    call assert_equal(l:tick, b:changedtick)
    call assert_equal(l:modified, &modified)
    call assert_equal(l:register, getreginfo('"'))
    call assert_equal(l:search, @/)

    call s:Move(1, 1)
    call feedkeys('v$y', 'xt')
    call assert_equal(l:source[0], substitute(@", '\n\+$', '', ''),
          \ 'Visual yanks original TeX')
    call feedkeys("ggvjj\<Cmd>doautocmd CursorMoved\<Bar>redraw!\<Bar>call assert_equal([], ".expand('<SID>')."Properties())\<CR>\<Esc>", 'xt')
    call s:Move(3, 1)

    let l:firstwin = win_getid()
    split
    call s:Move(1, 10)
    call win_execute(l:firstwin, 'let g:math_other_screen = join('.expand('<SID>').'View(), "\n")')
    call assert_match('before \$z_a\^2\$ and ν after', g:math_other_screen,
          \ 'source being edited is revealed consistently in both splits')
    call s:Move(3, 1)
    close
    call assert_equal(l:firstwin, win_getid())

    " Edits move untouched formulas with the text; touched ones become source.
    call setline(3, 'changed prose')
    call s:Block(['before za² and ν after', 'Next Q* end.', 'changed prose'])
    call append(0, 'new paragraph')
    call listener_flush()
    call assert_equal(2, s:Properties()[0].lnum, 'formulas follow inserted lines')
    call s:Block(['new paragraph', 'before za² and ν after', 'Next Q* end.'])
    call setline(3, 'Next, prefix $Q^\*$ end.')
    call listener_flush()
    call s:Block(['Next, prefix Q* end.'])
    call s:Move(4, 1)
    call setline(2, 'before $v^{r+1}+\beta$ after')
    call listener_flush()
    call assert_equal(1, len(b:texinsitu.formulas), 'only untouched math remains')
    call assert_match('before \$v\^{r+1}+\\beta\$ after', join(s:View(), "\n"))
    doautocmd TextChanged
    call s:Wait()
    call s:Block(['before vr+1+β after'])

    " Typing in Insert mode only shifts the line being edited; no reparse.
    call s:Move(3, 1)
    let l:tick = b:changedtick
    call feedkeys("Imore \<Cmd>call assert_equal(".l:tick.", b:texinsitu.parsed)\<CR>"
          \ ."\<Cmd>redraw!\<Bar>call ".expand('<SID>')."Block(['more Next, prefix Q* end.'])\<CR>\<Esc>", 'xt')
    call s:Wait()
    call s:Block(['more Next, prefix Q* end.'])

    let l:tick = b:changedtick
    call assert_false(texinsitu#toggle())
    call assert_match('before \$v\^{r+1}+\\beta\$ after', join(s:View(), "\n"))
    call assert_equal([], s:Properties())
    call assert_equal(l:tick, b:changedtick)
    call assert_true(texinsitu#toggle())
    call s:Wait()
    call s:Block(['before vr+1+β after'])

    " Wrapped prose stays real text: no reflowed copy and no blank rows after it.
    call s:Buffer([repeat('word ', 12).'value $A_{\mathrm h}=12$, conjugate $Q^\*$ and '
          \ .'change $\Delta u_\sigma$ in '.repeat('more ', 10).'end.', 'after'], 'tex')
    let l:source = getline(1, '$')
    let l:tick = b:changedtick
    set columns=40
    doautocmd VimResized
    let l:view = s:View()
    call assert_notmatch('\$', join(l:view, "\n"), 'wrapped source is replaced once')
    call assert_match('Ah=12', join(l:view, ''))
    let l:last = match(l:view, 'end\.$')
    call assert_equal('after', l:view[l:last + 1], 'no reserved blank rows: '.string(l:view))
    call s:Move(1, 1)
    call assert_equal(3, len(b:texinsitu.shown),
          \ 'the line being edited keeps the formulas the cursor is not in')
    set columns=100
    doautocmd VimResized
    call assert_equal(l:source, getline(1, '$'))
    call assert_equal(l:tick, b:changedtick)

    " Punctuation after rendered math moves with the formula, rather than
    " being stranded on a continuation of the concealed source.
    call s:Buffer(['A $64^\circ$ YX LiNbO$_3$ substrate with \SI{120}{nm} films '
          \ .'at fill ratio $0.4$ and $\ell=\SI{3.6}{\micro\metre}$.', 'after'], 'tex')
    setlocal linebreak breakindent
    let l:source = getline(1, '$')
    let l:tick = b:changedtick
    let l:parsed = b:texinsitu.parsed
    let l:breakat = &breakat
    for l:breaks in [" \t!@*-+;:,./?", " \t"]
      let &breakat = l:breaks
      for l:width in range(35, 100)
        execute 'set columns='.l:width
        doautocmd VimResized
        let l:view = s:View()
        let l:end = index(l:view, 'after')
        call assert_true(l:end > 0, string(l:view))
        call assert_equal([], filter(copy(l:view[:l:end - 1]),
              \ 'v:val =~# "^↳ *[.,;:!?]\\?$"'),
              \ 'no punctuation-only or blank continuation at '.l:width.': '.string(l:view))
        if l:width >= 40
          call assert_match('ℓ=3\.6\%u00a0μm\.$', l:view[l:end - 1], string(l:view))
        endif
        call assert_equal(l:source, getline(1, '$'))
        call assert_equal(l:tick, b:changedtick)
        call assert_equal(l:parsed, b:texinsitu.parsed, 'resize never reparses')
      endfor
    endfor
    let &breakat = " \t"
    set columns=75
    doautocmd VimResized
    call s:Move(1, match(getline(1), '\$\\ell') + 2)
    call assert_match('\\ell=', join(s:View(), "\n"), 'entering still reveals source')
    call s:Move(2, 1)
    call assert_notmatch('\\ell=', join(s:View(), "\n"))
    call setline(1, 'Prefix '.getline(1))
    call listener_flush()
    let l:view = s:View()
    call assert_match('ℓ=3\.6\%u00a0μm\.$', l:view[index(l:view, 'after') - 1],
          \ 'typing before the formula recomputes its padding')
    vertical split
    vertical resize 45
    doautocmd WinResized
    call assert_match('\\ell=', join(s:View(), "\n"),
          \ 'unequal views show source when they need different padding')
    close
    doautocmd WinResized
    call assert_notmatch('\\ell=', join(s:View(), "\n"))
    for l:indent in [0, 4]
      for l:punct in [',', '。', '),']
        call setline(1, repeat(' ', l:indent)
              \ .'Prefix $\ell=\SI{3.6}{\micro\metre}$'.l:punct)
        doautocmd TextChanged
        call s:Wait()
        for l:width in [40, 75]
          execute 'set columns='.l:width
          doautocmd VimResized
          let l:view = s:View()
          call assert_equal([], filter(copy(l:view[:index(l:view, 'after') - 1]),
                \ 'v:val =~# "^ *↳ *[.,;:!?。)]*$"'),
                \ 'indent and closing punctuation: '.string(l:view))
        endfor
      endfor
    endfor
    let &breakat = l:breakat
    setlocal nolinebreak nobreakindent
    set columns=100
    doautocmd VimResized

    " Entering either neighbour reflows the row without losing other math.
    for l:type in ['tex', 'markdown']
      call s:Buffer(['Prefix $a_i$, '.repeat('word ', 5).'$b^2$.', 'Other $\nu$.', 'after'], l:type)
      let l:source = getline(1, '$')
      let l:tick = b:changedtick
      for l:width in [40, 75, 100, 40, 100]
        execute 'set columns='.l:width
        doautocmd VimResized
        let l:other = filter(s:Properties(), 'v:val.lnum == 2')
        for l:pattern in ['\$a', '\$b']
          call s:Move(1, match(getline(1), l:pattern) + 2)
          let l:view = s:View()
          call assert_match(l:pattern, join(l:view, "\n"), 'entered neighbour shows source')
          call assert_match('Other ν\.', join(l:view, "\n"), 'other line stays rendered')
          call assert_equal(l:other, filter(s:Properties(), 'v:val.lnum == 2'))
          call s:Move(3, 1)
          let l:view = s:View()
          call assert_notmatch('\$', join(l:view, "\n"), 'both neighbours return')
          call assert_equal([], filter(l:view[:index(l:view, 'after') - 1],
                \ 'v:val =~# "^↳ *[.,;:!?]*$"'), string(l:view))
        endfor
      endfor
      call assert_equal(l:source, getline(1, '$'))
      call assert_equal(l:tick, b:changedtick)
      let l:other = filter(s:Properties(), 'v:val.lnum == 2')
      for l:prefix in ['Edit ', '']
        call setline(1, l:prefix.l:source[0])
        call listener_flush()
        call assert_notmatch('\$', join(s:View(), "\n"), 'prose edits keep both neighbours rendered')
        call assert_equal(l:other, filter(s:Properties(), 'v:val.lnum == 2'),
              \ 'same-line edits preserve properties on other lines')
      endfor
      call assert_equal(l:source, getline(1, '$'))
    endfor

    " Deleting an equation being edited must retire its reveal state too.
    call s:Buffer(['Before $x^2$.', '\begin{equation}',
          \ ' \frac{a}{b}=c', '\end{equation}', 'After $\nu$.', 'end'], 'tex')
    call s:Move(3, 3)
    2,4delete _
    call listener_flush()
    call s:Move(3, 1)
    call s:Block(['Before x².', 'After ν.', 'end'])

    " A narrow display restores exactly after revealing any of its lines.
    for l:type in ['tex', 'markdown']
      for l:width in [40, 75, 100]
        execute 'set columns='.l:width
        call s:Buffer(['Before $\nu$.', '\[',
              \ ' \left(\frac{a}{b}\right) % attachment crosses a comment',
              \ ' _{j}^{m}+q', '\]', 'after'], l:type)
        let l:rendered = s:View()
        let l:source = getline(1, '$')
        let l:tick = b:changedtick
        let l:other = filter(s:Properties(), 'v:val.lnum == 1')
        for l:lnum in range(2, 5)
          call s:Move(l:lnum, 1)
          call assert_match('\\frac{a}{b}', join(s:View(), "\n"), 'any block line reveals source')
          call assert_equal(l:other, filter(s:Properties(), 'v:val.lnum == 1'))
          call s:Move(6, 1)
          call assert_equal(l:rendered, s:View(), 'restored grid has no extra rows')
        endfor
        call assert_equal(l:source, getline(1, '$'))
        call assert_equal(l:tick, b:changedtick)
      endfor
    endfor

    call s:Buffer(['\newcommand{\statev}{\bm{\phi}}', '\begin{equation}',
          \ ' \widetilde P_{\mathrm{SRC}}+3\widetilde P_{\mathrm{aux}}\leq900',
          \ ' \label{eq:bound}', '\end{equation}', '\begin{samepage}',
          \ 'Text between equations.', '\begin{equation}',
          \ ' r_{ab}=\frac{\sum_{t=1}^{S}|H_a^*(\nu_t)|^2}{1+\widetilde\statev_b}+'
          \ .repeat('q+', 10).'q.',
          \ ' \label{eq:ratio}', '\end{equation}', '\end{samepage}', 'after'], 'tex')
    " Grid rows sit on the source lines they replace, centred among them.
    " Scripts stay on the baseline, so a formula without fractions is one row.
    " The label stands right of the last row, as an equation number does.
    call s:Block(['└ P̃SRC+3P̃aux≤900  eq:bound', '', '\begin{samepage}'])
    let l:view = s:View()
    call assert_notmatch('\\begin{equation}\|\\label{', join(l:view, "\n"),
          \ 'the complete equation is rendered, including its wrappers and label')
    " A long source line cannot also hold a grid row: the grid stays whole and
    " the remaining lines only continue its border down to the label.
    let l:bar = match(l:view, '─')
    call assert_true(l:bar > 0)
    call assert_match('S', join(l:view[: l:bar - 1], "\n"))
    let l:end = match(l:view, '└ eq:ratio')
    call assert_true(l:end > l:bar)
    call assert_equal([], filter(l:view[l:bar : l:end], 'v:val !~# "^[│└]"'),
          \ 'one continuous block: '.string(l:view[l:bar : l:end]))
    let l:tick = b:changedtick
    call s:Move(3, 12)
    call assert_match('\\begin{equation}', join(s:View(), "\n"))
    call assert_equal(l:tick, b:changedtick)
    call s:Move(13, 1)

    " A source line wider than the window keeps its own continuation row; the
    " block then ends on the following lines, with nothing reserved after it.
    call s:Buffer(['Intro.', '\begin{equation}',
          \ ' r_{ab}=\frac{\sum_{t=1}^{S}|\widetilde H(\nu_t;\widetilde\theta_{ab})-H_a^*(\nu_t)|^2}'
          \ .repeat(' ', 30), ' {\sum_{t=1}^{S}|H_a^*(\nu_t)|^2}.', ' \label{eq:wide}',
          \ '\end{equation}', 'after'], 'tex')
    let l:view = s:View()
    let l:end = match(l:view, '^└.*eq:wide$')
    call assert_equal('after', l:view[l:end + 1], 'no rows after the block: '.string(l:view))
    let l:wrap = match(l:view, '↳')
    call assert_true(l:wrap > 0 && l:wrap < match(l:view, '─'),
          \ 'the continuation row precedes the grid: '.string(l:view))
    call assert_equal([], filter(l:view[l:wrap + 1 : l:end], 'v:val !~# "^[│└]"'))
    call s:Move(8, 1)

    " A short grid over many source lines sits in their middle; the unused
    " lines stay blank, without border.
    call s:Buffer(['Intro.', '\begin{equation}', ' \Omega_{\mathrm{ok}}=',
          \ ' \left\{x\in\Omega:', ' \begin{aligned}', ' &a\in\mathbb{Z}^2,\\',
          \ ' &N_{\mathrm{in}}\leq500', ' \end{aligned}', ' \right\}.',
          \ ' \label{eq:set}', '\end{equation}', 'after'], 'tex')
    let l:view = s:View()
    let l:end = match(l:view, '^└ .*  eq:set$')
    call assert_match('^│ *⎰', l:view[l:end - 1], 'a two-row brace: '.string(l:view))
    call assert_equal(['Intro.'] + repeat([''], 4), l:view[: l:end - 2], string(l:view))
    call assert_equal(repeat([''], 4) + ['after'], l:view[l:end + 1 : l:end + 5], string(l:view))
    call s:Move(12, 1)

    " A phone-width window where one source line wraps: Vim keeps that line's
    " rows, so the grid sits below it and the label shares the last row
    " instead of pushing a row above the grid.
    set columns=75
    call s:Buffer(['Text.', '\begin{equation}',
          \ ' Z(\omega;\pvec)=\frac{U(\omega)}{W(\omega)}=Q_{22}^{\mathrm{all}}(\omega;\pvec),xx',
          \ ' \qquad \omega=2\pi f.', ' \label{eq:transfer-function}', '\end{equation}', 'after'], 'tex')
    let l:view = s:View()
    let l:end = match(l:view, 'W(ω)  eq:transfer-function$')
    call assert_true(l:end > 0, string(l:view))
    call assert_equal('after', l:view[l:end + 1], string(l:view))
    call assert_match('U(ω)', l:view[l:end - 2], string(l:view))
    call assert_equal(['Text.', '', '', '↳'], l:view[: l:end - 3], string(l:view))
    set columns=100
    call s:Move(7, 1)

    " Two requests in a row, each larger than a pipe, still both complete:
    " opening a long manuscript used to leave Vim and the renderer each
    " waiting for the other to read.
    call s:Buffer(map(range(3000), '"Term $a_{".v:val."}^2+\\frac{b}{c_{".v:val."}}$ end."'), 'tex')
    call texinsitu#clear(bufnr('%'))
    call texinsitu#enter()
    call texinsitu#clear(bufnr('%'))
    call texinsitu#enter()
    call s:Wait()
    call assert_equal(3000, len(b:texinsitu.formulas))

    for l:type in ['markdown', 'cogitatum']
      call s:Buffer(['Text $z_a$ end.', '`$code$` and \$5.',
            \ '```latex', '\begin{equation}', '$code^2$', '\end{equation}', '```',
            \ '$x+\custom{y}$ and $\frac{x}{$ and $\input{p}$', 'after'], l:type)
      call s:Block(['Text za end.'])
      let l:view = join(s:View(), "\n")
      call assert_match('`\$code\$` and \\\$5.', l:view)
      call assert_match('\$code\^2\$', l:view, 'fenced code remains literal')
      call assert_match('x+custom(y)', l:view,
            \ 'unknown call keeps its name and argument: '.l:view)
      call assert_match('\$\\frac{x}{\$ and \$\\input{p}\$', l:view,
            \ 'incomplete formulas remain entirely source')
      call s:Buffer(['Para', '$$', 'L=\frac{1}{N}\sum_{i=1}^{N}e_i', '$$', 'after'], l:type)
      call s:Block(['Para', '│   1  N'])
      call assert_equal('after', s:View()[match(s:View(), '∑') + 2])
    endfor

    " With 'ambiwidth' double, α and the rule ─ are two cells each (UAX #11):
    " the rule under αβ is two strokes, not four.
    set ambiwidth=double
    call s:Buffer(['Para', '\[', '\frac{\alpha\beta}{2}', '\]', 'after'], 'tex')
    call s:Block(['│ αβ', '│ ──', '│   2'])
    set ambiwidth=single
    call s:Buffer(['Para', '\[', '\frac{\alpha\beta}{2}', '\]', 'after'], 'tex')
    call s:Block(['│ αβ', '│ ──', '│  2'])

    call s:Buffer(['An accent: $\text{\''\i \^\j}$ end.', 'after'], 'tex')
    call assert_equal('An accent: í ĵ end.', s:View()[0])
    call s:Move(1, 15)
    call assert_match('\$\\text{', s:View()[0], 'an accented formula still edits as source')

    " The no-break space before a unit is not drawn as 'listchars' nbsp.
    call s:Buffer(['Film \SI{120}{nm} thick.', 'after'], 'tex')
    setlocal list listchars=nbsp:␣
    call assert_equal('Film 120'.nr2char(0xa0).'nm thick.', s:View()[0])

    call s:Buffer(['中文 $z^2$ tail $\nu$。', 'after'], 'markdown')
    call assert_equal([8, 19], map(filter(s:Properties(),
          \ 'v:val.type ==# "TexInsituInline"'), 'v:val.col'))
    call s:Move(1, 10)
    let l:source = getline(1, '$')
    let l:tick = b:changedtick
    runtime autoload/texinsitu.vim
    call assert_equal(l:source, getline(1, '$'))
    call assert_equal(l:tick, b:changedtick)
    call s:Move(2, 1)
    setlocal filetype=python
    call assert_equal([], s:Properties())
    call assert_equal([], filter(getmatches(), 'v:val.group ==# "Conceal"'))

    call mkdir(s:root, 'p')
    let l:file = s:root.'/refresh.md'
    call writefile(['Before $z^2$ after.', 'after'], l:file)
    execute 'edit! '.fnameescape(l:file)
    setlocal nonumber norelativenumber signcolumn=no foldcolumn=0 nofoldenable
    call s:Move(2, 1)
    call s:Wait()
    normal! ggA more
    doautocmd TextChanged
    call s:Move(2, 1)
    call s:Wait()
    normal! u
    doautocmd TextChanged
    call s:Wait()
    call assert_equal('Before $z^2$ after.', getline(1), 'undo changes source only')
    call texinsitu#toggle()
    normal! ggA again
    normal! u
    doautocmd TextChanged
    call assert_equal([], s:Properties())
    call texinsitu#toggle()
    call s:Move(2, 1)
    call s:Wait()
    call writefile(['Before $q_j$ after.', 'after'], l:file)
    edit!
    call s:Move(2, 1)
    call s:Wait()
    call s:Block(['Before qj after.'])
    call assert_false(&modified)
    call assert_equal(['Before $q_j$ after.', 'after'], readfile(l:file))

    " A child uses the in-memory main source, including unsaved macro changes.
    let l:main = s:root.'/main.tex'
    call writefile(['\newcommand{\statev}{\theta}', '\begin{document}',
          \ '\input{part}', '\end{document}'], l:main)
    let l:mainbuf = bufadd(l:main)
    noautocmd call bufload(l:mainbuf)
    enew!
    call setline(1, ['before $\statev_a$ after', 'after'])
    setfiletype tex
    setlocal nonumber norelativenumber signcolumn=no foldcolumn=0 nofoldenable
    let b:vimtex = {'tex': l:main}
    call s:Move(2, 1)
    doautocmd WinEnter
    call s:Wait()
    call s:Block(['before θa after'])
    call setbufline(bufnr(l:main), 1, '\newcommand{\statev}{\phi}')
    doautocmd WinEnter
    call s:Wait()
    call assert_notmatch('θ', join(s:View(), "\n"))
    call assert_match('ϕ', join(s:View(), "\n"))
    call assert_equal(['\newcommand{\statev}{\theta}', '\begin{document}',
          \ '\input{part}', '\end{document}'], readfile(l:main),
          \ 'reading macro context never saves the main source')
    unlet b:vimtex

    " Conceal belongs to the active window/buffer, never the user's globals.
    enew!
    setlocal conceallevel=1 concealcursor=i
    call setline(1, ['before $\alpha_{word}$ after', 'after'])
    setfiletype tex
    setlocal nonumber norelativenumber signcolumn=no foldcolumn=0 nofoldenable
    call s:Move(2, 1)
    call s:Wait()
    call s:Move(1, 12)
    call assert_equal('before $\alpha_{word}$ after', s:View()[0],
          \ 'entered math reveals source even with the default TeX syntax')
    call assert_false(exists('g:tex_conceal'))
    call assert_false(exists('g:vimtex_syntax_conceal_disable'))
    call assert_equal([2, 'nvic'], [&l:conceallevel, &l:concealcursor])
    call texinsitu#toggle()
    call assert_equal([1, 'i'], [&l:conceallevel, &l:concealcursor])
    call assert_equal([], s:Properties())
    call s:Move(2, 1)
    call assert_equal(1, synconcealed(1, 10)[0],
          \ 'turning rendering off restores native syntax conceal')
    call texinsitu#toggle()
    call s:Wait()
    " Changing syntax after enabling still leaves the entered formula raw.
    setlocal syntax=tex
    call s:Move(1, 12)
    call assert_equal('before $\alpha_{word}$ after', s:View()[0])
    call assert_false(exists('g:tex_conceal'))
    call assert_false(exists('g:vimtex_syntax_conceal_disable'))
    enew!
    call assert_equal([&g:conceallevel, &g:concealcursor],
          \ [&l:conceallevel, &l:concealcursor],
          \ 'new buffers keep their normal conceal options')

    let g:texinsitu_enabled = 0
    setlocal conceallevel=1 concealcursor=i
    call setline(1, ['before $\alpha_{word}$ after', 'after'])
    setfiletype tex
    call assert_true(empty(get(b:, 'texinsitu', {})))
    call assert_equal([1, 'i'], [&l:conceallevel, &l:concealcursor])
    call assert_false(exists('g:tex_conceal'))
    unlet g:texinsitu_enabled

    let g:tex_conceal = ''
    let g:vimtex_syntax_conceal_disable = 1
    let g:markdown_syntax_conceal = 1
    call s:Buffer(['before $z^2$ after', 'after'], 'tex')
    call texinsitu#toggle()
    call assert_equal('', g:tex_conceal)
    call assert_equal(1, g:vimtex_syntax_conceal_disable)
    call s:Buffer(['before $z^2$ after', 'after'], 'markdown')
    call texinsitu#toggle()
    call assert_equal(1, g:markdown_syntax_conceal)
    unlet g:tex_conceal g:vimtex_syntax_conceal_disable g:markdown_syntax_conceal
  catch
    call add(v:errors, v:exception.' at '.v:throwpoint)
  endtry
  call delete(s:root, 'rf')
  call writefile(empty(v:errors) ? ['PASS'] : ['FAIL'] + v:errors,
        \ s:result)
  if !empty(v:errors) | cquit | endif
  qall!
endfunction

augroup texinsitu_test
  autocmd!
  autocmd VimEnter * ++nested call <SID>Run()
augroup END
