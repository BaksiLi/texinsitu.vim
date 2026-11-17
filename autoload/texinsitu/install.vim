let s:root = expand('<sfile>:p:h:h:h')

function! s:Exec(arguments) abort
  let l:output = system(join(map(copy(a:arguments), 'shellescape(v:val)'), ' '))
  if v:shell_error
    throw a:arguments[0].' failed: '.trim(l:output)
  endif
  return trim(l:output)
endfunction

function! s:PSQuote(value) abort
  return "'".substitute(a:value, "'", "''", 'g')."'"
endfunction

function! s:PowerShell(lines, stage) abort
  let l:script = a:stage.'/install.ps1'
  call writefile(["\ufeff$ErrorActionPreference = 'Stop'"] + a:lines, l:script)
  return s:Exec(['powershell', '-NoProfile', '-NonInteractive',
        \ '-ExecutionPolicy', 'Bypass', '-File', l:script])
endfunction

function! s:Target(os, architecture) abort
  let l:arch = tolower(a:architecture)
  let l:arch = index(['arm64', 'aarch64'], l:arch) >= 0 ? 'aarch64'
        \ : index(['x86_64', 'amd64'], l:arch) >= 0 ? 'x86_64' : ''
  if a:os ==# 'Darwin' && !empty(l:arch)
    return l:arch.'-apple-darwin'
  elseif a:os ==# 'Linux' && !empty(l:arch)
    return l:arch.'-unknown-linux-musl'
  elseif a:os ==# 'Windows' && l:arch ==# 'x86_64'
    return 'x86_64-pc-windows-msvc'
  endif
  throw 'no prebuilt kernel for '.a:os.'/'.a:architecture
endfunction

function! s:Platform(stage) abort
  if has('win32')
    if !executable('powershell') | throw 'PowerShell is required' | endif
    let l:arch = s:PowerShell([
          \ 'if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 }',
          \ 'else { $env:PROCESSOR_ARCHITECTURE }'], a:stage)
    return s:Target('Windows', l:arch)
  endif
  return s:Target(s:Exec(['uname', '-s']), s:Exec(['uname', '-m']))
endfunction

function! s:Version() abort
  let l:package = 0
  for l:line in readfile(s:root.'/kernel/Cargo.toml')
    if l:line =~# '^\s*\['
      let l:package = l:line =~# '^\s*\[package\]\s*$'
    elseif l:package && l:line =~# '^\s*version\s*='
      let l:version = matchstr(l:line, '"\zs[^"]\+\ze"')
      if l:version =~# '^\d\+\.\d\+\.\d\+\%(-[0-9A-Za-z.-]\+\)\?\%(+[0-9A-Za-z.-]\+\)\?$'
        return l:version
      endif
      break
    endif
  endfor
  throw 'cannot read the kernel version from Cargo.toml'
endfunction

function! s:Download(url, path, stage) abort
  if has('win32')
    call s:PowerShell([
          \ '[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12',
          \ 'Invoke-WebRequest -UseBasicParsing -Uri '.s:PSQuote(a:url)
          \ .' -OutFile '.s:PSQuote(a:path)], a:stage)
  else
    call s:Exec(['curl', '--fail', '--location', '--silent', '--show-error',
          \ '--proto', '=https', '--proto-redir', '=https',
          \ '--connect-timeout', '15', '--max-time', '120',
          \ '--output', a:path, a:url])
  endif
endfunction

function! s:Prebuilt(stage, name) abort
  let l:target = s:Platform(a:stage)
  let l:version = s:Version()
  let l:archive = 'texinsitu-v'.l:version.'-'.l:target
        \ .(has('win32') ? '.zip' : '.tar.gz')
  let l:base = 'https://github.com/BaksiLi/texinsitu.vim/releases/download/v'.l:version.'/'
  if !has('win32')
    for l:tool in ['curl', 'tar', has('macunix') ? 'shasum' : 'sha256sum']
      if !executable(l:tool) | throw l:tool.' is required' | endif
    endfor
  endif
  call s:Download(l:base.'SHA256SUMS', a:stage.'/SHA256SUMS', a:stage)
  let l:hashes = []
  for l:line in readfile(a:stage.'/SHA256SUMS')
    let l:entry = matchlist(l:line, '^\(\x\{64}\) [ *]\(.\+\)$')
    if !empty(l:entry) && l:entry[2] ==# l:archive
      call add(l:hashes, tolower(l:entry[1]))
    endif
  endfor
  if len(l:hashes) != 1 | throw 'missing or duplicate checksum for '.l:archive | endif
  let l:path = a:stage.'/'.l:archive
  call s:Download(l:base.l:archive, l:path, a:stage)
  if has('win32')
    let l:actual = s:PowerShell([
          \ '(Get-FileHash -Algorithm SHA256 -LiteralPath '.s:PSQuote(l:path).').Hash'],
          \ a:stage)
  else
    let l:command = has('macunix') ? ['shasum', '-a', '256'] : ['sha256sum']
    let l:actual = matchstr(s:Exec(l:command + [l:path]), '^\x\{64}')
  endif
  if tolower(l:actual) !=# l:hashes[0] | throw 'checksum mismatch for '.l:archive | endif
  let l:unpacked = a:stage.'/unpacked'
  call mkdir(l:unpacked, '', 0700)
  if has('win32')
    call s:PowerShell([
          \ 'Expand-Archive -LiteralPath '.s:PSQuote(l:path)
          \ .' -DestinationPath '.s:PSQuote(l:unpacked)], a:stage)
  else
    call s:Exec(['tar', '-xzf', l:path, '-C', l:unpacked, a:name])
  endif
  return l:unpacked.'/'.a:name
endfunction

function! s:Source(stage, name) abort
  if !executable('cargo') | throw 'Rust and Cargo are required for a source build' | endif
  call s:Exec(['cargo', 'build', '--release', '--locked',
        \ '--manifest-path', s:root.'/kernel/Cargo.toml', '--target-dir', a:stage])
  return a:stage.'/release/'.a:name
endfunction

function! texinsitu#install#run(...) abort
  let l:mode = get(a:, 1, 'prebuilt')
  if a:0 > 1 || index(['prebuilt', 'source'], l:mode) < 0
    throw "TeXinsitu: use :TexInsituInstall or :call texinsitu#install#run('source')"
  endif
  let l:directory = s:root.'/kernel/target/release'
  let l:name = 'texinsitu'.(has('win32') ? '.exe' : '')
  let l:stage = l:directory.'/.texinsitu-install-'.fnamemodify(tempname(), ':t')
  try
    call mkdir(l:directory, 'p')
    call mkdir(l:stage, '', 0700)
    let l:candidate = l:mode ==# 'source'
          \ ? s:Source(l:stage, l:name) : s:Prebuilt(l:stage, l:name)
    if getftype(l:candidate) !=# 'file' || getfsize(l:candidate) <= 0
      throw 'the archive or build did not contain '.l:name
    endif
    if !has('win32') && !setfperm(l:candidate, 'rwxr-xr-x')
      throw 'cannot make the kernel executable'
    endif
    let l:destination = l:directory.'/'.l:name
    if has('win32')
      call s:PowerShell([
            \ '$source = '.s:PSQuote(l:candidate),
            \ '$destination = '.s:PSQuote(l:destination),
            \ 'if ([IO.File]::Exists($destination)) {',
            \ '  [IO.File]::Replace($source, $destination, $null)',
            \ '} else { [IO.File]::Move($source, $destination) }'], l:stage)
    elseif rename(l:candidate, l:destination)
      throw 'cannot replace the installed kernel'
    endif
  catch
    throw 'TeXinsitu: '.v:exception
          \ .". For a source build, call texinsitu#install#run('source') (requires Rust)."
  finally
    if isdirectory(l:stage) | call delete(l:stage, 'rf') | endif
  endtry
  echomsg 'TeXinsitu: installed '.l:destination
  if exists(':TexInsituInstall') == 2 | call texinsitu#enter() | endif
  return 1
endfunction
