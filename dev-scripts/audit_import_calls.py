#!/usr/bin/env python3
"""Find direct imported calls in known ARM/Thumb functions; no reachability claim."""
import argparse
import json
from pathlib import Path

from audit_ipa import MachO
from capstone import Cs, CS_ARCH_ARM, CS_MODE_ARM, CS_MODE_THUMB
from capstone.arm import ARM_OP_IMM


def audit(binary, imports):
    m = MachO(binary)
    if m.crypto and m.crypto[2]:
        raise ValueError('Encrypted executable')
    missing = {row['name'] for row in imports if row['status'] == 'missing_export'}
    stubs = {address: name for address, name in m.symbol_stubs().items() if name in missing}
    text = m.sections['__text']
    owners = {}
    for symbol in m.symbols:
        address = symbol['value'] & ~1
        if symbol['type'] & 0xe and text['address'] <= address < text['address'] + text['size']:
            owners.setdefault(address, {'name': symbol['name'], 'thumb': bool(symbol['value'] & 1)})
    for name, cls in m.classes().items():
        for key, sign in [('instance_methods', '-'), ('class_methods', '+')]:
            for method in cls[key]:
                owners[method['imp'] & ~1] = {
                    'name': f"{sign}[{name} {method['selector']}]", 'thumb': bool(method['imp'] & 1)}
    starts = sorted(owners)
    calls = []
    for i, start in enumerate(starts):
        end = starts[i + 1] if i + 1 < len(starts) else text['address'] + text['size']
        dis = Cs(CS_ARCH_ARM, CS_MODE_THUMB if owners[start]['thumb'] else CS_MODE_ARM)
        dis.detail = True
        offset = m.offset(start)
        for instruction in dis.disasm(m.b[offset:offset + end - start], start):
            if instruction.mnemonic not in ('bl', 'blx', 'b', 'b.w'):
                continue
            operand = instruction.operands[0]
            if operand.type == ARM_OP_IMM and operand.imm in stubs:
                calls.append({'symbol': stubs[operand.imm], 'address': hex(instruction.address),
                              'caller': owners[start]['name']})
    return {'scope': __doc__, 'callsites': calls,
            'missing_callable_imports': sorted(set(stubs.values())),
            'missing_imports_without_stub': sorted(missing - set(stubs.values()))}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('compiled_imports')
    parser.add_argument('output')
    args = parser.parse_args()
    result = audit(args.binary, json.loads(Path(args.compiled_imports).read_text()))
    Path(args.output).write_text(json.dumps(result, indent=2) + '\n')
    print(f"{len(result['callsites'])} direct call sites; "
          f"{len(result['missing_callable_imports'])} missing callable imports; "
          f"{len(result['missing_imports_without_stub'])} other missing imports")
