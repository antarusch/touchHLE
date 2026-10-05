#!/usr/bin/env python3
"""Conservative ARMv7 receiver audit. Requires capstone and audit_ipa.py.

Usage: python audit_receivers.py EXECUTABLE TOUCHHLE_SOURCE REPORT.json
Infers self and explicitly typed object ivars. Unknown receivers, super sends,
categories, return types and dynamic dispatch require separate review. A source
match proves registration only, not correct behavior or runtime reachability.
Also checks keyed decoding calls in host source against each archive reader:
framework code can send methods that do not appear in the game's executable.
Checks archive methods on referenced Foundation value and collection classes when the game uses
keyed archiving. These are coverage candidates, not proof of archive reachability.
"""
import argparse
import hashlib
import json
import re
import struct
from collections import Counter
from pathlib import Path

from audit_ipa import MachO, source_inventory
from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM, CS_MODE_THUMB
from capstone.arm import ARM_OP_REG, ARM_OP_IMM, ARM_OP_MEM


def audit(binary, source, runtime_metadata=None):
    m = MachO(binary)
    if m.crypto and m.crypto[2]:
        raise ValueError('Encrypted instructions cannot be audited; use the executable used by touchHLE.')
    guest = m.classes()
    host, _, texts = source_inventory(source)
    runtime_metadata = runtime_metadata or {}
    classrefs = {int(address, 16): name for address, name in runtime_metadata.get('classrefs', {}).items()}
    for name, superclass in runtime_metadata.get('superclasses', {}).items():
        if name in guest:
            guest[name]['superclass'] = superclass
    class_addresses = {cls['address']: name for name, cls in guest.items()}
    ivars = {}
    for name, cls in guest.items():
        ro = m.u32(cls['address'] + 16) & ~3
        table = m.u32(ro + 28)
        fields = {}
        if table:
            size, count = struct.unpack_from('<2I', m.b, m.offset(table))
            if size != 20 or count > 10000:
                raise ValueError('unsupported ivar table')
            for i in range(count):
                offset, _, encoding, _, _ = struct.unpack_from(
                    '<5I', m.b, m.offset(table + 8 + size * i))
                match = re.fullmatch(r'@"(\w+)"', m.cstr(encoding))
                if match:
                    fields[m.u32(offset)] = ('object', match[1], '-')
        ivars[name] = fields

    # Class clusters register instance methods on private implementation classes.
    # Recognize only explicit allocation redirects in the public implementation.
    redirects = {}
    for text in texts.values():
        for match in re.finditer(r'@implementation\s+(\w+)\s*:[^\n]+(.*?)@end', text, re.S):
            targets = set(re.findall(
                r'msg_class!\[env;\s*(\w+)\s+allocWithZone:', match[2]))
            if len(targets) == 1:
                redirects[match[1]] = targets.pop()

    def lookup(name, key):
        if key.startswith('-'):
            name = redirects.get(name, name)
        seen = set()
        while name not in seen:
            seen.add(name)
            if name in guest:
                field = 'class_methods' if key.startswith('+') else 'instance_methods'
                if any(method['selector'] == key[1:] for method in guest[name][field]):
                    return {'implementation_class': name, 'guest_defined': True}
                name = guest[name]['superclass']
                continue
            if name not in host:
                return None
            cls = host[name]
            if key in cls['methods']:
                return {'implementation_class': name, **cls['methods'][key]}
            name = cls['superclass']
        return None

    def inherits_coder(name):
        seen = set()
        while name in host and name not in seen:
            if name == 'NSCoder':
                return True
            seen.add(name)
            name = host[name]['superclass']
        return False

    readers = sorted(name for name, cls in host.items()
                     if '-decodeObjectForKey:' in cls['methods'] and inherits_coder(name))
    reader_calls = []
    for path, text in texts.items():
        # Explicit coder variables, restricted to the keyed reading selectors.
        # This includes host initWithCoder: and decoding helper implementations.
        clean = re.sub(r'^\s*//.*$', '', text, flags=re.M)
        for match in re.finditer(r'msg!\[env;\s*coder\s+((?:decode\w+|containsValueForKey):)', clean):
            selector = match[1]
            reader_calls.append({'source': path,
                                 'selector': selector,
                                 'readers': {name: lookup(name, '-' + selector) is not None
                                             for name in readers}})

    methods = [{**x, 'owner': n, 'kind': k} for n, c in guest.items()
               for k, field in [('-', 'instance_methods'), ('+', 'class_methods')]
               for x in c[field]]
    starts = sorted({s['value'] & ~1 for s in m.symbols
                     if s['section'] == 1 and s['value'] and s['type'] & 0xe}
                    | {x['imp'] & ~1 for x in methods})
    stubs = m.symbol_stubs()
    selectors = {m.cstr(p) for p in m.pointers('__objc_selrefs')}
    collection_coding = []
    if selectors & {'archivedDataWithRootObject:', 'encodeObject:forKey:'}:
        referenced = {s['name'].split('$_', 1)[1] for s in m.symbols
                      if s['name'].startswith('_OBJC_CLASS_$_')}
        referenced.update(field[1] for fields in ivars.values() for field in fields.values())
        collections = {'NSArray', 'NSMutableArray', 'NSDictionary', 'NSMutableDictionary',
                       'NSSet', 'NSMutableSet', 'NSCountedSet', 'NSString', 'NSMutableString',
                       'NSData', 'NSMutableData', 'NSDate', 'NSNumber', 'NSNull', 'NSValue',
                       'NSURL', 'NSIndexPath', 'NSError'}
        for name in sorted(referenced & collections):
            checks = {selector: lookup(name, '-' + selector) is not None
                      for selector in ['encodeWithCoder:', 'initWithCoder:']}
            collection_coding.append({'class': name, 'registrations': checks,
                                      'status': ('registrations_found' if all(checks.values())
                                                 else 'missing_foundation_coding_candidate')})
    category_selectors = set()
    for addr in m.pointers('__objc_catlist'):
        for field, kind in [(8, '-'), (12, '+')]:
            category_selectors.update(kind + x['selector'] for x in m.methods(m.u32(addr + field)))
    rows = []
    for method in methods:
        start = method['imp'] & ~1
        end = next((s for s in starts if s > start), start + 512)
        if end - start > 20000:
            continue
        thumb = bool(method['imp'] & 1)
        dis = Cs(CS_ARCH_ARM, CS_MODE_THUMB if thumb else CS_MODE_ARM)
        dis.detail = True
        code = m.b[m.offset(start):m.offset(start) + end - start]
        instructions = list(dis.disasm(code, start))
        # At every branch destination discard path-specific register facts.
        targets = {o.imm for ins in instructions
                   if ins.group(1) and ins.mnemonic not in ('bl', 'blx')
                   for o in ins.operands if o.type == ARM_OP_IMM}
        regs = {'r0': ('object', method['owner'], method['kind'])}
        stack = {}
        if method['kind'] == '-' and method['selector'] == 'initWithCoder:':
            regs['r2'] = ('coder', 'NSCoder', '-')
        for ins in instructions:
            if ins.address in targets and ins.address != start:
                regs.clear()
                stack.clear()
            values = dict(regs)
            op = ins.operands
            pc = (ins.address + 4) & ~3 if thumb else ins.address + 8

            def register(reg):
                name = ins.reg_name(reg)
                return pc if name == 'pc' else values.get(name)

            def value(operand):
                if operand.type == ARM_OP_IMM:
                    return operand.imm
                if operand.type == ARM_OP_REG:
                    return register(operand.reg)
                return None

            try:
                _, writes = ins.regs_access()
                for reg in writes:
                    regs.pop(ins.reg_name(reg), None)
                    if ins.reg_name(reg) == 'sp':
                        stack.clear()
                result = None
                if ins.mnemonic in ('mov', 'movs', 'movw') and len(op) == 2:
                    result = value(op[1])
                elif ins.mnemonic == 'movt' and len(op) == 2:
                    old = register(op[0].reg)
                    if isinstance(old, int):
                        result = (old & 0xffff) | (op[1].imm << 16)
                elif ins.mnemonic in ('add', 'adds', 'sub', 'subs') and len(op) in (2, 3):
                    a = register(op[0].reg) if len(op) == 2 else value(op[1])
                    b = value(op[-1])
                    if isinstance(a, int) and isinstance(b, int):
                        result = (a + b if ins.mnemonic.startswith('add') else a - b) & 0xffffffff
                elif ins.mnemonic in ('ldr', 'ldr.w') and len(op) == 2 and op[1].type == ARM_OP_MEM:
                    mem = op[1].mem
                    base = register(mem.base)
                    index = register(mem.index) if mem.index else 0
                    if isinstance(index, int) and not op[1].shift.value:
                        offset = index + mem.disp
                        if ins.reg_name(mem.base) == 'sp' and not mem.index:
                            result = stack.get(mem.disp)
                        elif isinstance(base, int):
                            address = (base + offset) & 0xffffffff
                            if address in classrefs:
                                result = ('class', classrefs[address], '+')
                            else:
                                result = m.u32(address)
                                if result in class_addresses:
                                    result = ('class', class_addresses[result], '+')
                        elif isinstance(base, tuple) and base[2] == '-':
                            result = ivars.get(base[1], {}).get(offset)
                elif ins.mnemonic in ('str', 'str.w') and len(op) == 2 and op[1].type == ARM_OP_MEM:
                    mem = op[1].mem
                    if ins.reg_name(mem.base) == 'sp' and not mem.index:
                        stack[mem.disp] = value(op[0])
                elif ins.mnemonic in ('bl', 'blx'):
                    for name in ('r0', 'r1', 'r2', 'r3', 'r12'):
                        regs.pop(name, None)
                    symbol = stubs.get(value(op[0]))
                    if symbol and symbol.startswith('_objc_msgSend'):
                        stret = symbol.endswith('_stret')
                        pointer = values.get('r2' if stret else 'r1')
                        receiver = values.get('r1' if stret else 'r0')
                        selector = m.cstr(pointer) if isinstance(pointer, int) else None
                        row = {'owner': method['owner'], 'method': method['selector'],
                               'address': hex(ins.address), 'selector': selector,
                               'status': 'unresolved_receiver_or_selector'}
                        if selector in selectors and isinstance(receiver, tuple):
                            _, name, kind = receiver
                            row.update(receiver_class=name, method_kind=kind)
                            if receiver[0] == 'coder':
                                checks = {reader: lookup(reader, kind + selector) is not None
                                          for reader in readers}
                                row['possible_readers'] = checks
                                row['status'] = ('archive_reader_registrations_found' if all(checks.values())
                                                 else 'missing_archive_reader_method_candidate')
                            elif name in host or name in guest:
                                found = lookup(name, kind + selector)
                                row['status'] = ('guest_method_found' if found and found.get('guest_defined') else
                                                 'host_registration_found' if found else
                                                 'missing_host_method_candidate' if name in host else
                                                 'guest_receiver_requires_review')
                                if found:
                                    row['implementation'] = found
                                elif kind + selector in category_selectors:
                                    # Do not mark a guest category as an absent host API.
                                    # The category's target class still requires review.
                                    row['status'] = 'guest_category_requires_review'
                            else:
                                row['status'] = 'guest_receiver_requires_review'
                            if not stret:
                                if selector in ('alloc', 'allocWithZone:', 'new') and kind == '+':
                                    regs['r0'] = ('object', name, '-')
                                elif selector == 'mainBundle' and name == 'NSBundle':
                                    regs['r0'] = ('object', 'NSBundle', '-')
                                elif selector == 'class':
                                    regs['r0'] = ('class', name, '+')
                        rows.append(row)
                elif ins.mnemonic in ('b', 'b.w', 'bx') or ins.mnemonic.startswith('pop'):
                    regs.clear()
                    stack.clear()
                if result is not None:
                    regs[ins.reg_name(op[0].reg)] = result
            except (ValueError, IndexError, struct.error):
                # Unreadable literals leave the affected register unknown.
                pass
    return {'scope': __doc__, 'binary_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
            'counts': dict(Counter(x['status'] for x in rows)), 'callsites': rows,
            'host_reader_calls': reader_calls,
            'foundation_coding': collection_coding,
            'host_reader_missing': sorted({(reader, row['selector']) for row in reader_calls
                                           for reader, found in row['readers'].items() if not found})}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('source')
    parser.add_argument('report')
    parser.add_argument('--runtime-metadata', help='Class references and superclasses emitted by the opt-in original-game test')
    args = parser.parse_args()
    metadata = json.loads(Path(args.runtime_metadata).read_text()) if args.runtime_metadata else None
    report = audit(args.binary, args.source, metadata)
    Path(args.report).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['counts'], indent=2))
    for row in report['callsites']:
        if row['status'] in ('missing_host_method_candidate', 'missing_archive_reader_method_candidate'):
            print(row)
    print('Host decoding calls:', len(report['host_reader_calls']))
    print('Missing reader registrations:', report['host_reader_missing'])
    print('Foundation archive coverage:', report['foundation_coding'])
