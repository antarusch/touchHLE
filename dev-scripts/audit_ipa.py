#!/usr/bin/env python3
"""Static dependency inventory; source matches are candidates, not runtime proof."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path


class MachO:
    def __init__(self, path):
        self.path = path
        self.b = Path(path).read_bytes()
        self.sections = {}
        self.libs = []
        self.symbols = []
        self.crypto = None
        if self.b[:4] == bytes.fromhex('cafebabe'):
            count, = struct.unpack_from('>I', self.b, 4)
            arches = [struct.unpack_from('>5I', self.b, 8 + 20 * i) for i in range(count)]
            arch = next(a for a in arches if a[0] == 12 and a[1] & 255 == 9)
            self.b = self.b[arch[2]:arch[2] + arch[3]]
        header = struct.unpack_from('<7I', self.b)
        assert header[0] == 0xfeedface and header[1] == 12
        self.header = header
        pos = 28
        symtab = None
        self.indirect = []
        for _ in range(header[4]):
            cmd, size = struct.unpack_from('<2I', self.b, pos)
            if cmd == 1:
                seg = struct.unpack_from('<16s8I', self.b, pos + 8)
                for i in range(seg[7]):
                    sec = struct.unpack_from('<16s16s9I', self.b, pos + 56 + 68 * i)
                    self.sections[sec[0].rstrip(b'\0').decode()] = {
                        'segment': sec[1].rstrip(b'\0').decode(), 'address': sec[2],
                        'size': sec[3], 'offset': sec[4], 'flags': sec[8],
                        'reserved1': sec[9], 'reserved2': sec[10],
                    }
            elif cmd == 2:
                symtab = struct.unpack_from('<4I', self.b, pos + 8)
            elif cmd == 11:
                vals = struct.unpack_from('<18I', self.b, pos + 8)
                self.indirect = list(struct.unpack_from('<' + 'I' * vals[13], self.b, vals[12]))
            elif cmd in (12, 0x80000018, 0x8000001f):
                nameoff, = struct.unpack_from('<I', self.b, pos + 8)
                self.libs.append(self.string_offset(pos + nameoff))
            elif cmd == 0x21:
                self.crypto = struct.unpack_from('<3I', self.b, pos + 8)
            pos += size
        if symtab:
            symoff, nsym, stroff, _ = symtab
            for i in range(nsym):
                strx, kind, section, desc, value = struct.unpack_from('<IBBHI', self.b, symoff + 12 * i)
                name = self.string_offset(stroff + strx) if strx else ''
                ordinal = desc >> 8
                self.symbols.append({'name': name, 'type': kind, 'section': section,
                    'desc': desc, 'value': value,
                    'library': self.libs[ordinal - 1] if 0 < ordinal <= len(self.libs) else None})

    def symbol_stubs(self):
        result = {}
        for sec in self.sections.values():
            if sec['flags'] & 0xff != 0x8:  # S_SYMBOL_STUBS
                continue
            stride = sec['reserved2']
            assert stride and sec['size'] % stride == 0
            for i in range(sec['size'] // stride):
                index = self.indirect[sec['reserved1'] + i]
                if index & 0xc0000000:  # INDIRECT_SYMBOL_LOCAL / ABS
                    continue
                result[sec['address'] + stride * i] = self.symbols[index]['name']
        return result

    def string_offset(self, offset):
        end = self.b.find(b'\0', offset)
        if end < 0:
            raise ValueError('unterminated string')
        return self.b[offset:end].decode('utf-8', 'replace')

    def offset(self, addr):
        for sec in self.sections.values():
            if sec['address'] <= addr < sec['address'] + sec['size'] and sec['offset']:
                return sec['offset'] + addr - sec['address']
        raise ValueError(f'unmapped address {addr:#x}')

    def u32(self, addr):
        return struct.unpack_from('<I', self.b, self.offset(addr))[0]

    def cstr(self, addr):
        return self.string_offset(self.offset(addr)) if addr else ''

    def pointers(self, name):
        sec = self.sections.get(name)
        if not sec:
            return []
        return list(struct.unpack_from('<' + 'I' * (sec['size'] // 4), self.b, sec['offset']))

    def methods(self, addr):
        if not addr:
            return []
        entsize, count = struct.unpack_from('<2I', self.b, self.offset(addr))
        entsize &= 0xffff
        assert entsize == 12 and count < 10000, (entsize, count)
        result = []
        for i in range(count):
            name, types, imp = struct.unpack_from('<3I', self.b, self.offset(addr + 8 + entsize * i))
            result.append({'selector': self.cstr(name), 'types': self.cstr(types), 'imp': imp})
        return result

    def classes(self):
        classes = {}
        by_addr = {}
        for addr in self.pointers('__objc_classlist'):
            data = self.u32(addr + 16) & ~3
            name = self.cstr(self.u32(data + 16))
            meta = self.u32(addr)
            mdata = self.u32(meta + 16) & ~3
            classes[name] = {'address': addr, 'super_address': self.u32(addr + 4),
                'instance_methods': self.methods(self.u32(data + 20)),
                'class_methods': self.methods(self.u32(mdata + 20))}
            by_addr[addr] = name
        for value in classes.values():
            value['superclass'] = by_addr.get(value['super_address'])
        return classes


def source_inventory(root):
    classes = {}
    exports = {}
    all_text = {}
    for path in sorted(Path(root, 'src').rglob('*.rs')):
        if path.name == 'api_tests.rs':
            continue
        text = path.read_text()
        rel = str(path.relative_to(root))
        all_text[rel] = text
        # Remove full comment lines before identifying registered exports.
        clean = re.sub(r'^\s*//.*$', '', text, flags=re.M)
        for match in re.finditer(r'export_c_func!\(\s*(\w+)\s*\(', clean):
            exports['_' + match[1]] = rel
        for match in re.finditer(r'export_c_func_aliased!\(\s*"([^"]+)"', clean):
            exports['_' + match[1]] = rel
        for match in re.finditer(r'"(_[\w$]+)"\s*,', clean):
            exports.setdefault(match[1], rel)
        for cm in re.finditer(r'@implementation\s+(\w+)(?:[ \t]*:[ \t]*(\w+))?(.*?)(?=@end)', clean, re.S):
            name, superclass, body = cm.groups()
            entry = classes.setdefault(name, {'superclass': superclass, 'methods': {}})
            matches = list(re.finditer(r'^\s*([+-])\s*\(((?:[^()]|\([^()]*\))*)\)\s*([^{};]+?)\s*\{', body, re.M))
            for i, mm in enumerate(matches):
                signature = re.sub(r'/\*.*?\*/|//[^\n]*', '', mm[3], flags=re.S)
                stripped = re.sub(r':\s*\((?:[^()]|\([^()]*\))*\)\s*\w+', ':', signature)
                stripped = re.sub(r',\s*\.\.\.\w+', '', stripped)
                selector = re.sub(r'\s+', '', stripped)
                if not re.fullmatch(r'[\w:]+', selector):
                    raise ValueError((rel, signature, selector))
                start = mm.end(); depth = 1; end = start
                # Sufficient for brace-balanced Rust method bodies; strings may
                # contain braces, so flags only indicate methods needing review.
                while end < len(body) and depth:
                    if body[end] == '{': depth += 1
                    elif body[end] == '}': depth -= 1
                    end += 1
                method_body = body[start:end - 1]
                flags = []
                if re.search(r'\b(?:todo|unimplemented)!', method_body): flags.append('panic_placeholder')
                if 'TODO' in method_body: flags.append('todo_review')
                entry['methods'][mm[1] + selector] = {'source': rel, 'flags': flags}
    return classes, exports, all_text


def nib_class_inventory(directory, guest_classes, host_classes):
    files = []
    for path in sorted(Path(directory).glob('*.nib')):
        data = path.read_bytes()
        if not data.startswith(b'NIBArchive'):
            files.append({'file':path.name, 'format':'unparsed'})
            continue
        fields = struct.unpack_from('<10I', data, 10)
        pos = fields[9]
        def integer():
            nonlocal pos
            result = 0; shift = 0
            for _ in range(5):
                byte = data[pos]; pos += 1
                result |= (byte & 127) << shift
                if byte & 128:
                    return result
                shift += 7
            raise ValueError('bad NIB integer')
        names = []
        for _ in range(fields[8]):
            length = integer(); extra = integer(); pos += extra * 4
            name = data[pos:pos + length].rstrip(b'\0').decode(); pos += length
            names.append({'name':name,'status':'guest_defined' if name in guest_classes else
                'host_defined' if name in host_classes else 'no_definition_found'})
        assert pos == len(data), (path, pos, len(data))
        files.append({'file':path.name,'format':'NIBArchive','classes':names})
    return files


def main():
    ap = argparse.ArgumentParser(); ap.add_argument('binary'); ap.add_argument('source'); ap.add_argument('out')
    args = ap.parse_args()
    m = MachO(args.binary)
    guest = m.classes()
    host, exports, texts = source_inventory(args.source)
    guest_dylib_exports = {}
    for path in Path(args.source, 'touchHLE_dylibs').glob('*.dylib'):
        if path.name not in [Path(lib).name for lib in m.libs]:
            continue
        try:
            lib = MachO(path)
        except (AssertionError, StopIteration, ValueError):
            continue
        for symbol in lib.symbols:
            if symbol['type'] & 0xe != 0 and symbol['type'] & 1:
                guest_dylib_exports[symbol['name']] = str(path.name)
    undefined = [s for s in m.symbols if s['type'] & 0xe == 0 and s['type'] & 1]
    selectors = sorted(set(m.cstr(p) for p in m.pointers('__objc_selrefs')))
    guest_methods = {x['selector'] for cls in guest.values() for kind in ['instance_methods','class_methods'] for x in cls[kind]}
    categories = []
    for addr in m.pointers('__objc_catlist'):
        name = m.cstr(m.u32(addr)); methods = m.methods(m.u32(addr + 8)) + m.methods(m.u32(addr + 12))
        categories.append({'name': name, 'methods': methods})
        guest_methods.update(x['selector'] for x in methods)
    host_methods = {key[1:] for cls in host.values() for key in cls['methods']}
    objc_names = sorted(set(s['name'].split('$_',1)[1] for s in undefined if s['name'].startswith('_OBJC_CLASS_$_')))
    imported = []
    for s in undefined:
        if s['name'].startswith('_OBJC_'):
            continue
        symbol = s['name']
        status = ('source_export_found' if symbol in exports else
            'bundled_guest_library_export' if symbol in guest_dylib_exports else
            'linker_special_case' if symbol in ['___CFConstantStringClassReference','dyld_stub_binder'] else
            'review_unresolved')
        imported.append({**s, 'source_match': exports.get(symbol),
            'guest_library_match': guest_dylib_exports.get(symbol), 'candidate_status': status})
    rows = []
    for selector in selectors:
        methods = [{'class': name, 'kind': key[0], **v} for name, cls in host.items() for key,v in cls['methods'].items() if key[1:] == selector]
        status = ('guest_defined' if selector in guest_methods else
            'host_method_candidate' if methods else 'no_method_definition_found')
        rows.append({'selector': selector, 'status': status, 'host_candidates': methods})
    output = {
        'scope': 'Static metadata inventory of the supplied binary; not full runtime coverage.',
        'binary_sha256': hashlib.sha256(Path(args.binary).read_bytes()).hexdigest(),
        'encryption_info': m.crypto, 'libraries': m.libs, 'sections': m.sections,
        'guest_classes': guest, 'guest_categories': categories,
        'nib_files': nib_class_inventory(Path(args.binary).parent, guest, host),
        'referenced_framework_classes': [{'name':n,'host_definition':n in host} for n in objc_names],
        'imports': imported, 'selectors': rows,
        'counts': {'selector_references':len(selectors),'guest_classes':len(guest),
            'host_class_definitions':len(host), 'framework_class_references':len(objc_names),
            'non_objc_imports':len(imported),
            'imports_without_source_match':sum(x['source_match'] is None for x in imported),
            'imports_to_review':sum(x['candidate_status']=='review_unresolved' for x in imported),
            'selectors_without_definition':sum(x['status']=='no_method_definition_found' for x in rows)},
    }
    Path(args.out).write_text(json.dumps(output, indent=2))
    print(json.dumps(output['counts'],indent=2))
    print('Class references:',output['referenced_framework_classes'])
    print('Unmatched imports:',[x['name'] for x in imported if not x['source_match']])
    print('Unmatched selectors:',[x['selector'] for x in rows if x['status']=='no_method_definition_found'])


if __name__ == '__main__':
    main()
