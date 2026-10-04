#!/usr/bin/env python3
"""Conservative ARMv7 receiver audit. Requires capstone and audit_ipa.py.

Usage: python audit_receivers.py EXECUTABLE TOUCHHLE_SOURCE REPORT.json
Infers self and explicitly typed object ivars. Unknown receivers, super sends,
categories, return types and dynamic dispatch require separate review. A source
match proves registration only, not correct behavior or runtime reachability.
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


def audit(binary, source):
    m = MachO(binary)
    if m.crypto and m.crypto[2]:
        raise ValueError('Encrypted instructions cannot be audited; use the executable used by touchHLE.')
    guest = m.classes()
    host, _, texts = source_inventory(source)
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
        while name in host and name not in seen:
            seen.add(name)
            cls = host[name]
            if key in cls['methods']:
                return {'implementation_class': name, **cls['methods'][key]}
            name = cls['superclass']
        return None

    methods = [{**x, 'owner': n, 'kind': k} for n, c in guest.items()
               for k, field in [('-', 'instance_methods'), ('+', 'class_methods')]
               for x in c[field]]
    starts = sorted({s['value'] & ~1 for s in m.symbols
                     if s['section'] == 1 and s['value'] and s['type'] & 0xe}
                    | {x['imp'] & ~1 for x in methods})
    sec = m.sections['__symbolstub1']
    stubs = {sec['address'] + 4 * i:
             m.symbols[m.indirect[sec['reserved1'] + i]]['name']
             for i in range(sec['size'] // 4)}
    selectors = {m.cstr(p) for p in m.pointers('__objc_selrefs')}
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
        for ins in instructions:
            if ins.address in targets and ins.address != start:
                regs.clear()
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
                        if isinstance(base, int):
                            result = m.u32((base + offset) & 0xffffffff)
                        elif isinstance(base, tuple) and base[2] == '-':
                            result = ivars.get(base[1], {}).get(offset)
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
                            if name in host:
                                found = lookup(name, kind + selector)
                                row['status'] = 'host_registration_found' if found else 'missing_host_method_candidate'
                                if found:
                                    row['implementation'] = found
                                elif kind + selector in category_selectors:
                                    # Do not mark a guest category as an absent host API.
                                    # The category's target class still requires review.
                                    row['status'] = 'guest_category_requires_review'
                            else:
                                row['status'] = 'guest_receiver_requires_review'
                        rows.append(row)
                elif ins.mnemonic in ('b', 'b.w', 'bx') or ins.mnemonic.startswith('pop'):
                    regs.clear()
                if result is not None:
                    regs[ins.reg_name(op[0].reg)] = result
            except (ValueError, IndexError, struct.error):
                # Unreadable literals leave the affected register unknown.
                pass
    return {'scope': __doc__, 'binary_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
            'counts': dict(Counter(x['status'] for x in rows)), 'callsites': rows}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('source')
    parser.add_argument('report')
    args = parser.parse_args()
    report = audit(args.binary, args.source)
    Path(args.report).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report['counts'], indent=2))
    for row in report['callsites']:
        if row['status'] == 'missing_host_method_candidate':
            print(row)
