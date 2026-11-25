#!/usr/bin/env python3
"""Concurrent file-tool regression through two real MCP adapters and one real broker.

Uses only disposable owned fixtures below --work-root. No running service, installation,
account, or user file is touched. Each pair of calls is released by a barrier; the run passes
when every check holds and every scenario overlapped at least once across the rounds. How
many pairs overlapped is a measurement, not the criterion. --expect-race is a diagnostic for a
binary without the broker's mutation gate: it must observe two same-SHA writes both succeed,
and is never a passing product gate. No OS-wide CAS, external-writer exclusion, durability, or
retry-idempotency claim is made (engine/machine-engine/src/broker/files.rs).
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import tempfile
import threading
import time


INITIALIZE_VERSION = '2025-11-25'


def require(value, message):
    if not value:
        raise AssertionError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def pump(pipe, output):
    try:
        for line in pipe:
            output.put(line)
    finally:
        output.put(None)


class Mcp:
    def __init__(self, binary, endpoint, env, log):
        self.log = log.open('w', encoding='utf-8')
        self.process = subprocess.Popen(
            [str(binary), 'mcp', '--socket', endpoint], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.log, text=True, encoding='utf-8',
            bufsize=1, env=env)
        self.lines = queue.Queue()
        threading.Thread(target=pump, args=(self.process.stdout, self.lines), daemon=True).start()
        self.sequence = 0
        self.rpc('initialize', {'protocolVersion': INITIALIZE_VERSION, 'capabilities': {},
                 'clientInfo': {'name': 'file-mutation-concurrency', 'version': '1'}})
        self.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + '\n')
        self.process.stdin.flush()

    def rpc(self, method, params):
        self.sequence += 1
        self.send({'jsonrpc': '2.0', 'id': self.sequence, 'method': method, 'params': params})
        deadline = time.monotonic() + 30
        while True:
            line = self.lines.get(timeout=max(.01, deadline - time.monotonic()))
            require(line is not None, 'MCP adapter exited unexpectedly')
            response = json.loads(line)
            if response.get('id') == self.sequence:
                require('error' not in response, f'JSON-RPC error: {response}')
                return response['result']

    def call(self, name, arguments):
        result = self.rpc('tools/call', {'name': name, 'arguments': arguments})
        body = result.get('structuredContent')
        if body is None:
            body = json.loads(result['content'][0]['text'])
        return {'ok': not result.get('isError', False), 'body': body}

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(10)
        self.log.close()


class Harness:
    def __init__(self, args):
        self.binary = args.devagotchi.resolve()
        self.work = Path(tempfile.mkdtemp(prefix='file-mutations-', dir=args.work_root.resolve()))
        self.workspace = self.work / 'workspace'
        self.workspace.mkdir()
        self.env = {k: v for k, v in os.environ.items() if not k.upper().startswith('DEVAGOTCHI_')}
        self.env.update(TEMP=str(self.work), TMP=str(self.work), TMPDIR=str(self.work))
        self.broker = None
        self.clients = []
        self.log = None
        self.pool = ThreadPoolExecutor(max_workers=2)
        self.receipt = {'started_at_utc': datetime.now(timezone.utc).isoformat(),
                        'source_commit': args.source_commit,
                        'binary': str(self.binary), 'binary_sha256': sha(self.binary.read_bytes()),
                        'platform': sys.platform, 'work': str(self.work), 'rounds': args.rounds,
                        'diagnostic': args.expect_race, 'interval_clock': 'perf_counter_ns',
                        'clock_resolution_secs': time.get_clock_info('perf_counter').resolution, 'cases': []}

    def start(self):
        config = {'workspace': str(self.workspace), 'exec_policy': 'granted',
                  'allowed_executables': [], 'max_operations': 8, 'max_concurrent': 4}
        path = self.work / 'broker.json'
        path.write_text(json.dumps(config), encoding='utf-8')
        self.log = (self.work / 'broker.log').open('w', encoding='utf-8')
        self.broker = subprocess.Popen(
            [str(self.binary), 'broker', 'run', '--state-dir', str(self.work / 'state'),
             '--config', str(path)], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE, text=True, encoding='utf-8', env=self.env)
        ready = queue.Queue()

        def read_broker():
            for line in self.broker.stderr:
                self.log.write(line)
                self.log.flush()
                if 'ready:' in line:
                    ready.put(line.split('ready:', 1)[1].strip())
            ready.put(None)

        threading.Thread(target=read_broker, daemon=True).start()
        endpoint = ready.get(timeout=30)
        require(endpoint is not None, 'Broker did not become ready')
        for index in range(2):
            self.clients.append(Mcp(self.binary, endpoint, self.env, self.work / f'mcp-{index}.log'))

    def race(self, label, calls):
        barrier = threading.Barrier(2)

        def call(index):
            barrier.wait(timeout=5)
            start = time.perf_counter_ns()
            result = self.clients[index].call(*calls[index])
            return dict(result, client=index, start_ns=start, end_ns=time.perf_counter_ns())

        futures = [self.pool.submit(call, index) for index in range(2)]
        results = [future.result(timeout=40) for future in futures]
        # Fast calls can finish before the scheduler runs the other released thread.
        # Keep every attempt; prove overlap per scenario across the declared run rather
        # than moving timestamps before the barrier or silently discarding samples.
        overlaps = max(r['start_ns'] for r in results) < min(r['end_ns'] for r in results)
        self.receipt['cases'].append({'case': label, 'overlapped': overlaps, 'results': results})
        return results

    def stop(self):
        for client in self.clients:
            client.stop()
        if self.broker and self.broker.poll() is None:
            self.broker.terminate()
            try:
                self.broker.wait(10)
            except subprocess.TimeoutExpired:
                self.broker.kill()
                self.broker.wait(10)
        self.pool.shutdown(wait=True)
        if self.log:
            time.sleep(.05)
            self.log.close()
        self.receipt['finished_at_utc'] = datetime.now(timezone.utc).isoformat()
        self.receipt['binary_sha256_end'] = sha(self.binary.read_bytes())
        changed = self.receipt['binary_sha256_end'] != self.receipt['binary_sha256']
        self.receipt['binary_changed'] = changed
        if changed:
            self.receipt['status'] = 'failed'
            self.receipt['failure'] = 'Candidate binary changed during the run'
        (self.work / 'receipt.json').write_text(json.dumps(self.receipt, indent=2) + '\n', encoding='utf-8',
                                                newline='\n')
        print('RECEIPT ' + str(self.work / 'receipt.json'), flush=True)
        require(not changed, 'Candidate binary changed during the run')


def one_winner(results, code):
    winners = [r['client'] for r in results if r['ok']]
    require(len(winners) == 1, f'Expected exactly one winner: {results}')
    loser = results[1 - winners[0]]
    actual = loser['body'].get('code', loser['body'].get('error', {}).get('code'))
    require(actual == code, f'Expected {code}, got {loser}')
    return winners[0]


def verify_write(path, result, expected):
    require(path.read_bytes() == expected, 'Final bytes differ from the acknowledged winner')
    require(result['body']['sha256'] == sha(expected), 'Winner checksum differs from its effect')
    require(result['body']['size'] == len(expected), 'Winner byte count differs from its effect')


def guarded_pair(h, iteration, mode='overwrite', absent=False, diagnostic=False):
    path = h.workspace / f'{mode}-{absent}-{iteration}.txt'
    seed = b'old-value\n' + b'x' * (4 * 1024 * 1024)
    if not absent:
        path.write_bytes(seed)
    payloads = [f'client-{i}-round-{iteration}\n'.encode() for i in range(2)]
    calls = [('write_file', {'path': str(path), 'text': payload.decode(), 'mode': mode,
                            'expected_sha256': '' if absent else sha(seed)}) for payload in payloads]
    result = h.race(f'guarded-{mode}-absent-{absent}-{iteration}', calls)
    if diagnostic and all(r['ok'] for r in result):
        require(h.receipt['cases'][-1]['overlapped'], 'Dual success without measured overlap')
        require(path.read_bytes() in payloads, 'Baseline final file is not either submitted value')
        return True
    winner = one_winner(result, 'changed_since_read')
    expected = (seed if mode == 'append' and not absent else b'') + payloads[winner]
    verify_write(path, result[winner], expected)
    return False


def other_cases(h, iteration):
    # Two create requests must not both claim creation of the same file.
    path = h.workspace / f'create-{iteration}.txt'
    values = [f'create-{i}'.encode() for i in range(2)]
    result = h.race(f'create-{iteration}', [('write_file', {'path': str(path), 'text': v.decode(),
                                   'mode': 'create'}) for v in values])
    winner = one_winner(result, 'already_exists')
    require(result[winner]['body']['created'], 'Successful create must report created')
    verify_write(path, result[winner], values[winner])

    # Distinct exact edits must both survive, rather than one replacing a stale snapshot.
    path = h.workspace / f'edit-disjoint-{iteration}.txt'
    seed = b'left-OLD\nright-OLD\n' + b'x' * (1024 * 1024)
    path.write_bytes(seed)
    result = h.race(f'disjoint-edits-{iteration}', [('edit_file', {'path': str(path),
                  'old_text': side + '-OLD', 'new_text': side + '-NEW'}) for side in ['left', 'right']])
    require(all(r['ok'] for r in result), f'Disjoint edits failed: {result}')
    expected = seed.replace(b'-OLD', b'-NEW')
    require(path.read_bytes() == expected, 'A successful edit was lost')
    require(any(r['body']['sha256'] == sha(expected) for r in result), 'No edit acknowledged final content')

    # A shared exact-text precondition can only be consumed once.
    path = h.workspace / f'edit-same-{iteration}.txt'
    path.write_bytes(seed)
    values = [seed.replace(b'left-OLD', f'left-{i}'.encode()) for i in range(2)]
    result = h.race(f'same-edits-{iteration}', [('edit_file', {'path': str(path),
                  'old_text': 'left-OLD', 'new_text': f'left-{i}'}) for i in range(2)])
    winner = one_winner(result, 'edit_mismatch')
    verify_write(path, result[winner], values[winner])

    # An edit and a SHA-guarded write contend in the same owner, not separate locks.
    path = h.workspace / f'mixed-{iteration}.txt'
    path.write_bytes(seed)
    values = [b'whole replacement', seed.replace(b'left-OLD', b'left-NEW')]
    result = h.race(f'mixed-write-edit-{iteration}', [
        ('write_file', {'path': str(path), 'text': values[0].decode(), 'expected_sha256': sha(seed)}),
        ('edit_file', {'path': str(path), 'old_text': 'left-OLD', 'new_text': 'left-NEW'})])
    require(sum(r['ok'] for r in result) == 1, f'Mixed mutation lost update: {result}')
    winner = one_winner(result, 'edit_mismatch' if result[0]['ok'] else 'changed_since_read')
    verify_write(path, result[winner], values[winner])

    # Moves compete on destination existence; the losing source must stay intact.
    destination = h.workspace / f'moved-{iteration}.txt'
    sources = [h.workspace / f'source-{iteration}-{i}.txt' for i in range(2)]
    for i, source in enumerate(sources):
        source.write_bytes(values[i])
    result = h.race(f'moves-{iteration}', [('move_file', {'source': str(source),
                  'destination': str(destination)}) for source in sources])
    winner = one_winner(result, 'already_exists')
    require(not sources[winner].exists() and destination.read_bytes() == values[winner], 'Wrong move effect')
    require(sources[1-winner].read_bytes() == values[1-winner], 'Rejected move changed its source')

    # mkdir remains idempotent; unconditional appends both retain their bytes and own receipts.
    directory = h.workspace / f'directories-{iteration}' / 'nested'
    result = h.race(f'mkdir-{iteration}', [('create_directory', {'path': str(directory)})] * 2)
    require(all(r['ok'] for r in result) and directory.is_dir(), 'Concurrent mkdir failed')
    path = directory / 'append.txt'
    values = [b'append-A\n', b'append-B\n']
    result = h.race(f'appends-{iteration}', [('write_file', {'path': str(path), 'text': value.decode(),
                       'mode': 'append'}) for value in values])
    require(all(r['ok'] for r in result), f'Unconditional append failed: {result}')
    final = path.read_bytes()
    require(final in [values[0] + values[1], values[1] + values[0]], 'Append lost or interleaved bytes')
    first = 0 if final.startswith(values[0]) else 1
    require(result[first]['body']['sha256'] == sha(values[first]), 'First append receipt included later mutation')
    require(result[1-first]['body']['sha256'] == sha(final), 'Last append receipt has wrong hash')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--devagotchi', type=Path, required=True)
    parser.add_argument('--work-root', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=16)
    parser.add_argument('--expect-race', action='store_true')
    parser.add_argument('--source-commit', help='the commit the binary was built from, for the receipt')
    args = parser.parse_args()
    require(args.rounds > 0, '--rounds must be positive')
    h = Harness(args)
    try:
        h.start()
        if args.expect_race:
            found = any(guarded_pair(h, i, diagnostic=True) for i in range(args.rounds))
            require(found, 'No race reproduced; baseline diagnostic is inconclusive')
            h.receipt['status'] = 'baseline-race-reproduced'
            print('BASELINE reproduced: two same-SHA writes both succeeded', flush=True)
        else:
            for i in range(args.rounds):
                guarded_pair(h, i)
                guarded_pair(h, i, mode='append')
                guarded_pair(h, i, absent=True)
                other_cases(h, i)
            overlap_by_scenario = {}
            for case in h.receipt['cases']:
                scenario = case['case'].rsplit('-', 1)[0]
                overlap_by_scenario[scenario] = overlap_by_scenario.get(scenario, 0) + int(case['overlapped'])
            h.receipt['overlap_by_scenario'] = overlap_by_scenario
            require(all(count > 0 for count in overlap_by_scenario.values()),
                    f'Inconclusive scheduling: a scenario never overlapped: {overlap_by_scenario}')
            h.receipt['status'] = 'passed'
            overlapping = sum(overlap_by_scenario.values())
            print(f'PASS {len(h.receipt["cases"])} barrier-released pairs; {overlapping} overlapped, '
                  f'including every scenario, through two real MCP clients', flush=True)
    except BaseException as error:
        h.receipt['status'] = 'failed'
        h.receipt['failure'] = str(error)
        raise
    finally:
        h.stop()


if __name__ == '__main__':
    main()
