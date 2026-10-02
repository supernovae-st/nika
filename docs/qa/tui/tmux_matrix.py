"""Real-terminal checks of the Nika TUI reception (docs/qa/tui/RECEPTION.md) through tmux.

    python3 docs/qa/tui/tmux_matrix.py --binary <nika-tui-proto> --out <dir> [--case NAME ...] [--list]

Every case runs on private tmux servers (-L nika-tui-qa-matrix and, for the Esc cases, -L nika-tui-qa-matrix-outer;
-f /dev/null: never the user's server or configuration). A case writes <out>/<case>/: one <label>.txt
(capture-pane -p), <label>.joined.txt (-pJ, wrapped lines joined: what a copy gives) and <label>.ansi (-pe) per
capture, then case.json (binary, tmux version, options, every step with its UTC time, every check with its
verdict). Write-once: an existing case directory refuses.

A check marked `known` encodes a defect already reported at the base (the affected component is named): it is recorded and
shown, and does not fail the case. Exit status 1 when any other check failed.

The demo journey is the one the PTY suites walk (crates/nika-tui/tests/qa_support/mod.rs JOURNEY).
"""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import shlex
import subprocess
import sys
import time

SOCKET = 'nika-tui-qa-matrix'
# The outer server of the Esc cases: a real terminal (tmux) whose pane runs `tmux attach` on the inner server, so
# the keys reach the inner server the way a human's do, through its input parser (escape-time and extended-keys
# apply to what a client types; send-keys on the inner server would bypass them).
OUTER = 'nika-tui-qa-matrix-outer'
TMUX = 'tmux'

# Seconds after a client attaches before the Esc cases type: past tmux's terminal-query window.
STEADY = 7.0

INTENT = 'digest my monday notes'
QUESTION = 'Which file holds the notes to digest?'
PROPOSAL_ID = '9f3c1a'
GATE = 'overwrite the existing file?'
BOX_VERTICAL = ('│', '┃', '║')

TO_GATE = [
    {'literal': INTENT}, {'keys': ['Enter']}, {'await': QUESTION},
    {'literal': './notes/lundi.md'}, {'keys': ['Enter']}, {'await': PROPOSAL_ID},
    {'literal': 'yes'}, {'keys': ['Enter']}, {'await': 'saved ./digest-notes.nika'},
    {'literal': 'run it'}, {'keys': ['Enter']}, {'await': GATE},
]


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def run_tmux(socket, *args, check=True):
    return subprocess.run([TMUX, '-L', socket, '-f', '/dev/null', *args], capture_output=True, text=True,
                          check=check)


def tmux(*args, check=True):
    return run_tmux(SOCKET, *args, check=check)


def outer(*args, check=True):
    return run_tmux(OUTER, *args, check=check)


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


class Case:
    """One tmux session running one command, its steps and its captures."""

    def __init__(self, name, out, binary):
        self.name = name
        self.dir = out / name
        self.dir.mkdir(parents=True, exist_ok=False)
        self.binary = binary
        self.session = 'qa-' + name
        self.target = self.session
        self.log = []
        self.checks = []
        self.captures = {}
        self.nested = False

    def start(self, size, args, env=None, options=(), split=None):
        """A detached session of `size` (COLSxROWS) whose pane runs the binary; `split` = columns of a
        right-hand pane split off, so the binary's pane is the real remainder of a tmux split."""
        cols, rows = size.split('x')
        tmux('kill-session', '-t', self.session, check=False)
        env = dict(env or {})
        env.setdefault('TERM', 'xterm-256color')
        command = 'env -u NO_COLOR ' + ' '.join(k + '=' + shlex.quote(v) for k, v in env.items())
        command += ' ' + ' '.join(shlex.quote(c) for c in [self.binary, *args])
        command += '; echo "EXIT=$?"; sleep 600'
        tmux('new-session', '-d', '-s', self.session, '-x', cols, '-y', rows, 'sleep 600')
        for option in options:
            tmux(*option)
        if split:
            tmux('split-window', '-h', '-t', self.session, '-l', str(split), 'sleep 600')
            self.target = self.session + '.0'
        tmux('respawn-pane', '-k', '-t', self.target, 'bash --noprofile --norc -c ' + shlex.quote(command))
        self.note({'start': size, 'args': args, 'env': env, 'options': [list(o) for o in options],
                   'split': split, 'pane': self.pane_size()})

    def pane_size(self):
        return tmux('display', '-p', '-t', self.target, '#{pane_width}x#{pane_height}').stdout.strip()

    def alternate_on(self):
        return tmux('display', '-p', '-t', self.target, '#{alternate_on}').stdout.strip() == '1'

    def note(self, step):
        self.log.append({'at': now(), 'step': step})

    def capture(self, label):
        plain = tmux('capture-pane', '-p', '-t', self.target, check=False).stdout
        joined = tmux('capture-pane', '-pJ', '-t', self.target, check=False).stdout
        ansi = tmux('capture-pane', '-pe', '-t', self.target, check=False).stdout
        (self.dir / (label + '.txt')).write_text(plain)
        (self.dir / (label + '.joined.txt')).write_text(joined)
        (self.dir / (label + '.ansi')).write_text(ansi)
        self.captures[label] = {'txt_sha256': sha(plain), 'joined_sha256': sha(joined), 'ansi_sha256': sha(ansi),
                                'alternate_on': self.alternate_on(), 'pane': self.pane_size()}
        self.note({'capture': label})
        return plain, joined

    def run(self, steps):
        for step in steps:
            if 'wait' in step:
                time.sleep(step['wait'])
            elif 'keys' in step:
                tmux('send-keys', '-t', self.target, *step['keys'], check=False)
            elif 'literal' in step:
                tmux('send-keys', '-t', self.target, '-l', step['literal'], check=False)
            elif 'await' in step:
                step = dict(step, found=self.wait_for(step['await'], step.get('timeout', 10)))
            elif 'resize' in step:
                cols, rows = step['resize'].split('x')
                tmux('resize-window', '-t', self.session, '-x', cols, '-y', rows)
            elif 'capture' in step:
                self.capture(step['capture'])
            self.note(step)

    def wait_for(self, needle, timeout):
        deadline = time.time() + timeout
        while time.time() < deadline:
            joined = tmux('capture-pane', '-pJ', '-S', '-200', '-t', self.target, check=False).stdout
            if needle in joined:
                return True
            time.sleep(0.05)
        return False

    def check(self, label, ok, detail='', known=None):
        """One verdict; `known` names the defect already reported at the base (it does not fail the case)."""
        self.checks.append({'check': label, 'pass': bool(ok), 'detail': detail, 'known': known})

    def attach_nested(self, cols, rows, settle=STEADY):
        """A real terminal for a real client: an outer tmux whose pane runs `tmux attach` on the inner server.
        tmux holds a lone Esc for at least 500 ms, whatever escape-time says, while its queries to a newly
        attached terminal are unanswered (measured on tmux 3.6b: 567-579 ms 1.5 s after attach at escape-time
        10, 127 ms 7 s after); `settle` waits that window out unless a case measures it."""
        outer('kill-session', '-t', self.session, check=False)
        attach = 'env TERM=xterm-256color ' + ' '.join(
            shlex.quote(c) for c in [TMUX, '-L', SOCKET, '-f', '/dev/null', 'attach', '-t', self.session])
        outer('new-session', '-d', '-s', self.session, '-x', str(cols), '-y', str(rows), attach)
        outer('set', '-t', self.session, 'status', 'off')
        self.nested = True
        time.sleep(settle)
        clients = tmux('list-clients', '-t', self.session, check=False).stdout.strip()
        self.note({'nested_client': clients, 'settled': settle})

    def type_nested(self, text, literal=True, pause=0.0):
        """What a human types into the outer terminal: it reaches the inner server as client input."""
        outer('send-keys', '-t', self.session, *(['-l'] if literal else []), text)
        self.note({'typed': text, 'literal': literal, 'pause': pause})
        time.sleep(pause)

    def finish(self):
        tmux('kill-session', '-t', self.session, check=False)
        if self.nested:
            outer('kill-session', '-t', self.session, check=False)
        verdict = all(c['pass'] or c['known'] for c in self.checks)
        version = subprocess.run([TMUX, '-V'], capture_output=True, text=True).stdout.strip()
        receipt = {'case': self.name, 'at': now(), 'binary': self.binary, 'tmux': version,
                   'steps': self.log, 'captures': self.captures, 'checks': self.checks, 'pass': verdict}
        (self.dir / 'case.json').write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + '\n')
        known = [c['check'] for c in self.checks if not c['pass'] and c['known']]
        return verdict, known


def lines_with_box_glyphs(text):
    return [line for line in text.splitlines() if any(g in line for g in BOX_VERTICAL)]


def case_split(case, width, presentation):
    """A real tmux split of a 24-row terminal: the status line takes one row (the window is 23 rows), the split
    border one column, so the binary's pane is `width` x 23."""
    args = ['--focus'] if presentation == 'focus' else []
    case.start('160x23', args, split=159 - width)
    case.run([{'await': 'nika ›'}] + TO_GATE + [{'wait': 0.3}, {'capture': 'gate'}])
    plain, _ = case.capture('gate-again')
    case.check('pane size', case.pane_size() == f'{width}x23', case.pane_size())
    case.check('gate question visible whole', GATE in plain)
    case.check('gate prompt visible', 'answer ›' in plain)
    case.check('gate hint visible', 'nothing else answers a gate' in plain)
    history = tmux('capture-pane', '-p', '-S', '-200', '-t', case.target, check=False).stdout
    case.check('the check layers never split across rows (RUN READY on one row)', 'RUN READY' in history,
               known='D3 check bar · terminal rendering')


def case_resize_joined(case, presentation):
    """120x40 -> 80x24 with a proposal on screen: what a copy gives keeps identity and paths whole."""
    args = ['--focus'] if presentation == 'focus' else []
    case.start('120x40', args)
    case.run([{'await': 'nika ›'}] + TO_GATE[:6] + [{'wait': 0.3}, {'capture': 'proposal-120x40'},
                                                   {'resize': '80x24'}, {'wait': 0.8}])
    plain, joined = case.capture('proposal-80x24')
    history = tmux('capture-pane', '-pJ', '-S', '-500', '-t', case.target, check=False).stdout
    (case.dir / 'history.joined.txt').write_text(history)
    for token in (PROPOSAL_ID, './notes/lundi.md', './digest.md'):
        case.check('whole in the joined copy: ' + token, token in joined or token in history)
    boxed = lines_with_box_glyphs(joined)
    case.check('no vertical box glyph copied', not boxed, repr(boxed[:3]))


def escape_setup(case, escape_time, extended, settle=STEADY):
    options = [('set', '-s', 'escape-time', str(escape_time)),
               ('set', '-s', 'extended-keys', 'on' if extended else 'off'),
               ('set', '-g', 'status', 'off')]
    case.start('100x32', ['--focus'], options=options)
    case.run([{'await': 'nika ›'}])
    case.attach_nested(100, 32, settle)
    case.type_nested('draft', pause=0.4)
    case.capture('before')
    case.check('setup: focus (alternate screen) with the draft typed through the client',
               case.alternate_on() and 'nika › draft' in case.capture('before-check')[0])


def case_escape(case, escape_time, extended, gap):
    """Measurement. In focus a human presses Esc, then `x` `gap` seconds later, through a real client: was the
    full screen left, where did the `x` go? The pinned law (tests/pty_restore.rs) is Esc -> inline."""
    escape_setup(case, escape_time, extended)
    case.type_nested('Escape', literal=False, pause=gap)
    case.type_nested('x', pause=1.0)
    plain, _ = case.capture('after-esc-x')
    left = not case.alternate_on()
    x_typed = 'draftx' in plain or 'nika › x' in plain
    case.check('observed', True, f'full screen left: {left} · x shown as a character: {x_typed}')


def case_escape_startup(case, escape_time):
    """Measurement. A lone Esc 1.5 s after the client attached (inside tmux's terminal-query window): how long
    until the full screen is left."""
    escape_setup(case, escape_time, False, settle=1.5)
    start = time.time()
    case.type_nested('Escape', literal=False)
    left_after = None
    while time.time() - start < 3.0:
        if not case.alternate_on():
            left_after = round((time.time() - start) * 1000)
            break
        time.sleep(0.005)
    case.capture('after-esc')
    case.check('observed', True, f'full screen left after: {left_after} ms (None: not within 3 s)')


def case_split_arrow(case, escape_time, gap):
    """Measurement. An Up arrow whose bytes arrive `gap` seconds apart (a slow link splits ESC from `[A`): a
    mis-parse turns it into Esc + text, which leaves the full screen and leaks `[A` into the draft."""
    escape_setup(case, escape_time, False)
    case.type_nested('\x1b', pause=gap)
    case.type_nested('[A', pause=1.0)
    plain, _ = case.capture('after-split-arrow')
    left = not case.alternate_on()
    leaked = '[A' in plain
    case.check('observed', True, f'full screen left: {left} · "[A" leaked into the draft: {leaked}')


def case_term(case, term):
    case.start('80x24', [], env={'TERM': term})
    case.run([{'await': 'nika ›'}] + TO_GATE + [{'wait': 0.3}])
    plain, _ = case.capture('gate')
    case.check('gate visible under TERM=' + term, GATE in plain and 'answer ›' in plain)


CASES = {
    'split-79x23-inline': lambda c: case_split(c, 79, 'inline'),
    'split-80x23-inline': lambda c: case_split(c, 80, 'inline'),
    'split-79x23-focus': lambda c: case_split(c, 79, 'focus'),
    'split-80x23-focus': lambda c: case_split(c, 80, 'focus'),
    'resize-120-80-inline-joined': lambda c: case_resize_joined(c, 'inline'),
    'resize-120-80-focus-joined': lambda c: case_resize_joined(c, 'focus'),
    'esc-then-x-50ms-escape-time-500-ext-off': lambda c: case_escape(c, 500, False, 0.05),
    'esc-then-x-50ms-escape-time-10-ext-off': lambda c: case_escape(c, 10, False, 0.05),
    'esc-then-x-50ms-escape-time-500-ext-on': lambda c: case_escape(c, 500, True, 0.05),
    'esc-then-x-50ms-escape-time-10-ext-on': lambda c: case_escape(c, 10, True, 0.05),
    'esc-then-x-700ms-escape-time-500-ext-off': lambda c: case_escape(c, 500, False, 0.7),
    'esc-startup-window-escape-time-10': lambda c: case_escape_startup(c, 10),
    'split-arrow-50ms-escape-time-10': lambda c: case_split_arrow(c, 10, 0.05),
    'split-arrow-50ms-escape-time-500': lambda c: case_split_arrow(c, 500, 0.05),
    'term-tmux-256color': lambda c: case_term(c, 'tmux-256color'),
    'term-screen-256color': lambda c: case_term(c, 'screen-256color'),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--binary', help='the nika-tui-proto executable')
    parser.add_argument('--out', help='receipt directory (created)')
    parser.add_argument('--case', action='append', help='run only this case (repeatable)')
    parser.add_argument('--list', action='store_true')
    args = parser.parse_args()
    if args.list:
        print('\n'.join(CASES))
        return 0
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    summary = {}
    for name in args.case or CASES:
        case = Case(name, out, os.path.abspath(args.binary))
        try:
            CASES[name](case)
        finally:
            verdict, known = case.finish()
            summary[name] = {'pass': verdict, 'known': known}
        print(name, 'pass' if verdict else 'FAIL', ('known: ' + '; '.join(known)) if known else '')
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
    (out / ('matrix-' + stamp + '.json')).write_text(json.dumps(summary, indent=2) + '\n')
    return 0 if all(v['pass'] for v in summary.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
