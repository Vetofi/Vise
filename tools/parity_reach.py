
import functools
import hashlib
from pathlib import Path
import re
import subprocess

IDENT = r"[A-Za-z_]\w*"
WORD = re.compile(rf"\b({IDENT})\b")
MOD_DECLARATION = re.compile(rf"\bmod\s+({IDENT})\s*;")
INLINE_MOD = re.compile(rf"\bmod\s+({IDENT})\s*\{{")
PATH_ATTRIBUTE = re.compile(r'#\[\s*path\s*=\s*"([^"]+)"\s*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;$')
BRACE = re.compile(r"[{}]")
TEST_ONLY = re.compile(r"#\[\s*(?:cfg\s*\(\s*test\s*\)|test)\s*\]")
TEST_MODULE = re.compile(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;$")
SPLIT = re.compile(r"[{}()\[\];]")
ITEM = re.compile(rf"(?:pub(?:\s*\([^)]*\))?\s+)?(?:(?:unsafe|async|const|extern|default|auto)\s+)*"
                  rf"(fn|struct|enum|union|trait|type|const|static|impl|mod|use|macro_rules\s*!|extern\s+crate)"
                  rf"(?!\w)\s*(?:mut\s+)?({IDENT})?")
PATH_BEFORE = re.compile(rf"\b({IDENT})\s*::")
PATH_AFTER = re.compile(rf"::\s*({IDENT})")
MACRO_USE = re.compile(rf"\b({IDENT})\s*!\s*[(\[{{]")
MACRO_CALL = re.compile(rf"({IDENT}(?:\s*::\s*{IDENT})*)\s*!")
DEFINED_INSIDE = re.compile(rf"\b(?:fn|struct|enum|union|trait|type|const|static)\s+({IDENT})")
NESTED_IMPL = re.compile(r"^[ \t]*(?:unsafe\s+)?impl\b([^{;]*)\{", re.MULTILINE)
QUALIFIED = re.compile(rf"{IDENT}(?:\s*::\s*{IDENT})*")
GLOB_IMPORT = re.compile(r"\buse\s[^;]*::\s*\*")
FILE_NAME = re.compile(r"[\w-]+\.(?:wgsl|glsl|json|png|ttf|otf|svg|txt|toml|ogg|wav|nbt|bin|dat|csv|mcmeta|wit|wasm)\b")
TYPE_KINDS = {"struct", "enum", "union", "trait", "type", "macro_call"}
STD_NAMES = {"Display", "Error", "Read", "Write", "Seek", "BufRead", "Deref", "DerefMut", "Index", "IndexMut", "Add",
             "Sub", "Mul", "Div", "Rem", "Neg", "Not", "BitAnd", "BitOr", "BitXor", "Shl", "Shr", "AddAssign",
             "SubAssign", "MulAssign", "DivAssign", "RemAssign", "BitAndAssign", "BitOrAssign", "BitXorAssign",
             "Borrow", "BorrowMut", "Future", "Any", "FromStr", "Hasher", "BuildHasher", "Formatter", "Ordering",
             "Serialize", "Deserialize", "Serializer", "Deserializer", "Visitor", "Termination", "Sum", "Product",
             "Wake", "Cow", "Rc", "Arc", "Mutex", "RwLock", "Cell", "RefCell", "HashMap", "HashSet", "BTreeMap",
             "BTreeSet", "VecDeque"}
PRELUDE = {"Option", "Some", "None", "Result", "Ok", "Err", "Vec", "String", "Box", "ToString", "ToOwned", "Clone",
           "Copy", "Send", "Sync", "Sized", "Unpin", "Drop", "Fn", "FnMut", "FnOnce", "From", "Into", "TryFrom",
           "TryInto", "AsRef", "AsMut", "Default", "Eq", "PartialEq", "Ord", "PartialOrd", "Iterator", "Extend",
           "IntoIterator", "DoubleEndedIterator", "ExactSizeIterator", "FromIterator", "Debug", "Hash",
           "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128",
           "isize", "f32", "f64"}
STD_NAMES |= PRELUDE
KEYWORDS = {"crate", "self", "super", "Self", "pub", "in", "as", "use", "for", "where", "dyn", "mut", "const", "fn",
            "unsafe", "impl", "std", "core", "alloc", "let", "if", "else", "match", "return", "struct", "enum", "mod",
            "type", "static", "trait", "true", "false", "loop", "while", "move", "ref", "break", "continue", "async",
            "await", "extern", "union"}


def segments(code, start, end):
    found, begin, depth = [], start, 0
    for match in SPLIT.finditer(code, start, end):
        character = match.group()
        if character in "{([":
            depth += 1
        elif character in "})]":
            depth -= 1
            if character == "}" and depth == 0:
                found.append((begin, match.end()))
                begin = match.end()
        elif depth == 0:
            found.append((begin, match.end()))
            begin = match.end()
    if code[begin:end].strip():
        found.append((begin, end))
    return found


def skip_attributes(code, index, end):
    while True:
        while index < end and code[index].isspace():
            index += 1
        if not code.startswith("#", index) or not re.match(r"#!?\[", code[index:index + 3]):
            return index
        depth = 0
        for position in range(code.index("[", index), end):
            depth += {"[": 1, "]": -1}.get(code[position], 0)
            if depth == 0:
                index = position + 1
                break
        else:
            return end


def body_of(code, start, end):
    opening = code.find("{", start, end)
    closing = code.rfind("}", start, end)
    return (opening + 1, closing) if 0 <= opening < closing else None


def words(text):
    return set(WORD.findall(text)) - KEYWORDS


def qualified_words(text):
    return set(PATH_AFTER.findall(text)) - KEYWORDS


def path_words(text):
    return set(PATH_BEFORE.findall(text)) | set(PATH_AFTER.findall(text))


def parse_items(code, start, end, items):
    for begin, finish in segments(code, start, end):
        head = skip_attributes(code, begin, finish)
        text = code[head:finish]
        if not text.strip(" \n\t;") or TEST_ONLY.search(code[begin:head]):
            continue
        match = ITEM.match(text)
        kind = re.sub(r"\s+", "", match.group(1)) if match else None
        name = match.group(2) if match else None
        if kind == "mod":
            body = body_of(code, head, finish)
            if body:
                parse_items(code, body[0], body[1], items)
            continue
        if kind in ("externcrate", None) and not MACRO_CALL.match(text):
            if kind is None:
                items.append({"kind": "glue", "mentions": sorted(words(text)), "paths": sorted(path_words(text)), "qualified": sorted(qualified_words(text)), "invokes": sorted(set(MACRO_USE.findall(text)))})
            continue
        if kind is None:
            items.append({"kind": "macro_call", "names": sorted(set(DEFINED_INSIDE.findall(text))),
                          "macro": MACRO_CALL.match(text).group(1).split("::")[-1].strip(),
                          "mentions": sorted(words(text)), "paths": sorted(path_words(text)), "qualified": sorted(qualified_words(text)), "invokes": sorted(set(MACRO_USE.findall(text)))})
        elif kind == "use":
            head_word = re.match(rf"\s*(?:::\s*)?({IDENT})", text[match.end(1):])
            items.append({"kind": "use", "public": text.startswith("pub"), "names": sorted(words(text)),
                          "head": head_word.group(1) if head_word else "", "glob": bool(re.search(r"::\s*\*", text)),
                          "mentions": sorted(words(text)), "paths": sorted(words(text)), "qualified": sorted(words(text)),
                          "invokes": []})
        elif kind == "impl":
            body = body_of(code, head, finish)
            subject = code[head + match.end(1) - match.start(): body[0] - 1 if body else finish]
            inherent = not re.search(r"\bfor\b", impl_subject(subject)[1])
            if inherent and body:
                members = []
                for member_begin, member_end in segments(code, body[0], body[1]):
                    member = code[skip_attributes(code, member_begin, member_end):member_end]
                    found = ITEM.match(member)
                    members.append({"name": found.group(2) if found else None, "mentions": sorted(words(member)),
                                    "paths": sorted(path_words(member)), "qualified": sorted(qualified_words(member)), "invokes": sorted(set(MACRO_USE.findall(member)))})
                items.append({"kind": "impl", "subject": subject, "inherent": True, "members": members,
                              "mentions": sorted(words(subject)), "paths": sorted(path_words(subject)), "qualified": sorted(qualified_words(subject)), "invokes": sorted(set(MACRO_USE.findall(subject)))})
            else:
                items.append({"kind": "impl", "subject": subject, "inherent": False, "mentions": sorted(words(text)),
                              "paths": sorted(path_words(text)), "qualified": sorted(qualified_words(text)), "invokes": sorted(set(MACRO_USE.findall(text)))})
        else:
            kind = "macro" if kind.startswith("macro_rules") else kind
            items.append({"kind": kind, "names": [name] if name else [], "mentions": sorted(words(text)),
                          "defines": sorted(set(DEFINED_INSIDE.findall(text))) if kind == "macro" else [],
                          "paths": sorted(path_words(text)), "qualified": sorted(qualified_words(text)), "invokes": sorted(set(MACRO_USE.findall(text)))})
            if kind in ("fn", "const", "static"):
                for header in NESTED_IMPL.findall(text):
                    items.append({"kind": "impl", "subject": header, "inherent": False,
                                  "mentions": sorted(words(text)), "paths": sorted(path_words(text)), "qualified": sorted(qualified_words(text)), "invokes": sorted(set(MACRO_USE.findall(text)))})


def crate_roots(crate_dir):
    roots = [crate_dir / "src" / "lib.rs", crate_dir / "src" / "main.rs"]
    for folder in (crate_dir / "src" / "bin", crate_dir / "examples"):
        if folder.is_dir():
            roots += sorted(folder.glob("*.rs")) + sorted(folder.glob("*/main.rs"))
    return [root for root in roots if root.is_file()]


class SourceGraph:

    def __init__(self, root, rust_code, cache=None):
        self.root = Path(root)
        self.rust_code = rust_code
        self.cache = {} if cache is None else cache
        self.used = set()
        self.files = {}
        self.parent = {}
        self.module_name = {}
        self.roots = set()
        self.target_cache = {}
        self.workspace_heads = None
        self.test_only = set()
        self.tables = None
        self.root_of = {}
        self.results = {}
        for manifest in sorted((self.root / "crates").glob("*/Cargo.toml")):
            for crate_root in crate_roots(manifest.parent):
                path = self.relative(crate_root)
                self.roots.add(path)
                self.root_of[path] = path
                self.place(path, crate_root.parent, set())

    def entries(self, artifacts, rust):
        found = set()
        for kind, name, *rest in artifacts:
            selection = rest[1] if len(rest) > 1 else []
            packages = [value for flag, value in zip(selection, selection[1:]) if flag == "-p"] or \
                [manifest.parent.name for manifest in sorted((self.root / "crates").glob("*/Cargo.toml"))]
            roots = set()
            for package in packages:
                crate = self.root / "crates" / package
                targets = self.targets(crate)
                if kind in ("example", "examples"):
                    candidates = [path for target, (target_kind, path) in targets.items()
                                  if target_kind == "example" and (not name or target == name)]
                else:
                    candidates = [path for target, (target_kind, path) in targets.items()
                                  if target_kind == "bin" and (not name or target == name)]
                for path in candidates:
                    if path.endswith("/examples/probe/main.rs"):
                        folder = path.removesuffix("main.rs")
                        roots |= {dependency for dependency in rust if dependency.startswith(folder)
                                  and "/" not in dependency.removeprefix(folder).removeprefix("support/")}
                        if roots:
                            roots.add(path)
                    elif path in rust:
                        roots.add(path)
            if not roots:
                return None
            found |= roots
        return found or None

    def targets(self, crate):
        if crate in self.target_cache:
            return self.target_cache[crate]
        found = {}
        manifest = (crate / "Cargo.toml").read_text(encoding="utf-8") if (crate / "Cargo.toml").is_file() else ""
        package = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', manifest, re.MULTILINE | re.DOTALL)
        if (crate / "src" / "main.rs").is_file() and package:
            found[package.group(1)] = ("bin", self.relative(crate / "src" / "main.rs"))
        for kind, folder in (("bin", crate / "src" / "bin"), ("example", crate / "examples")):
            for path in sorted(folder.glob("*.rs")) + sorted(folder.glob("*/main.rs")):
                found[path.stem if path.name != "main.rs" else path.parent.name] = (kind, self.relative(path))
        for kind, table in re.findall(r"^\[\[(bin|example)\]\]([^\[]*)", manifest, re.MULTILINE):
            name = re.search(r'^name\s*=\s*"([^"]+)"', table, re.MULTILINE)
            path = re.search(r'^path\s*=\s*"([^"]+)"', table, re.MULTILINE)
            if name and path and (crate / path.group(1)).is_file():
                found[name.group(1)] = (kind, self.relative(crate / path.group(1)))
        self.target_cache[crate] = found
        return found

    def target(self, path):
        return self.root_of.get(path, path)

    def is_library(self, path):
        return self.target(path).endswith("/src/lib.rs")

    def relative(self, path):
        return Path(path).resolve().relative_to(self.root.resolve()).as_posix()

    def place(self, path, module_dir, seen):
        if path in seen:
            return
        seen.add(path)
        parsed = self.parse(path)
        if parsed is None:
            return
        for name, nesting, attribute, test in parsed["declarations"]:
            if attribute is not None:
                if nesting:
                    continue
                candidates = [(self.root / path).parent / attribute]
            else:
                folder = module_dir.joinpath(*nesting)
                candidates = [folder / f"{name}.rs", folder / name / "mod.rs"]
            for candidate in candidates:
                if candidate.is_file():
                    child = self.relative(candidate)
                    self.parent.setdefault(child, path)
                    self.module_name.setdefault(child, name)
                    self.root_of.setdefault(child, self.root_of[path])
                    if test or path in self.test_only:
                        self.test_only.add(child)
                    child_dir = candidate.parent if candidate.name == "mod.rs" or attribute is not None \
                        else candidate.parent / candidate.stem
                    self.place(child, child_dir, seen)
                    break

    def parse(self, path):
        if path in self.files:
            return self.files[path]
        try:
            data = (self.root / path).read_bytes()
        except OSError:
            self.files[path] = None
            return None
        digest = hashlib.sha256(data).hexdigest()
        self.used.add(digest)
        if digest in self.cache:
            self.files[path] = parsed = self.cache[digest]
            return parsed
        raw = data.decode("utf-8", errors="replace")
        code = self.rust_code(raw)
        opening = {match.end() - 1: match.group(1) for match in INLINE_MOD.finditer(code)}
        events = sorted([(match.start(), match) for match in BRACE.finditer(code)]
                        + [(match.start(), match) for match in MOD_DECLARATION.finditer(code)], key=lambda event: event[0])
        declarations, stack, depth = [], [], 0
        for index, match in events:
            if match.group() == "{":
                depth += 1
                if index in opening:
                    stack.append((opening[index], depth))
            elif match.group() == "}":
                if stack and stack[-1][1] == depth:
                    stack.pop()
                depth -= 1
            else:
                window = " ".join(raw[max(0, index - 300):match.end()].split())
                attribute = PATH_ATTRIBUTE.search(window)
                declarations.append([match.group(1), [name for name, _ in stack], attribute.group(1) if attribute else None,
                                     bool(TEST_MODULE.search(window))])
        items = []
        parse_items(code, 0, len(code), items)
        self.files[path] = parsed = {
            "declarations": declarations,
            "items": items,
            "glob": bool(GLOB_IMPORT.search(code)),
            "file_names": sorted(set(FILE_NAME.findall(raw))),
        }
        self.cache[digest] = parsed
        return parsed

    def saved_cache(self):
        return {digest: self.cache[digest] for digest in self.used if digest in self.cache}

    def requirement(self, path, item):
        if "requires" in item:
            return item["requires"]
        if self.workspace_heads is None:
            self.workspace_heads = {"crate", "self", "super"} | set(self.module_name.values()) | {
                manifest.parent.name.replace("-", "_") for manifest in (self.root / "crates").glob("*/Cargo.toml")}
        parsed = self.parse(path)
        defined = {name for other in parsed["items"] for name in other.get("names", ()) if other["kind"] != "use"}
        workspace_imports = {name for other in parsed["items"] if other["kind"] == "use"
                             and other["head"] in self.workspace_heads for name in other["names"]}
        parameters, subject = impl_subject(item["subject"])
        parts = []
        for part in re.split(r"\bfor\b", subject, maxsplit=1):
            names, own = set(), set()
            for match in QUALIFIED.finditer(part):
                segments_ = [segment.strip() for segment in match.group().split("::")]
                name = segments_[-1]
                if name in parameters or name in KEYWORDS:
                    continue
                if len(segments_) > 1:
                    if segments_[0] in self.workspace_heads:
                        names.add(name)
                elif name in defined:
                    own.add(name)
                elif name in workspace_imports or (parsed["glob"] and name not in STD_NAMES):
                    names.add(name)
            parts.append([sorted(own), sorted(names)])
        item["requires"] = parts
        return parts

    def index(self):
        if self.tables is not None:
            return self.tables
        idents = {manifest.parent.name.replace("-", "_"): manifest.parent.name
                  for manifest in (self.root / "crates").glob("*/Cargo.toml")}
        paths = [path for path in self.files if self.files[path] is not None and path not in self.test_only]
        sees = {}
        for path in paths:
            named = {word for item in self.files[path]["items"] for word in item["mentions"]} & idents.keys()
            sees.setdefault(crate_of(path), {crate_of(path)}).update(idents[name] for name in named)
        changed = True
        while changed:
            changed = False
            for crate, seen in sees.items():
                wider = set().union(*(sees.get(other, {other}) for other in seen))
                if not wider <= seen:
                    seen |= wider
                    changed = True
        seen_by = {}
        for crate, seen in sees.items():
            for other in seen:
                seen_by.setdefault(other, set()).add(crate)
        expands = {}
        for path in paths:
            for item in self.files[path]["items"]:
                if item["kind"] == "macro":
                    for name in item["names"]:
                        expands.setdefault(name, set()).update(item["defines"])
        names_of, by_name, by_file_name, by_type, by_module, children, seeds = {}, {}, {}, {}, {}, {}, []
        for path in paths:
            crate = crate_of(path)
            if path in self.module_name:
                by_module.setdefault((crate, self.module_name[path]), []).append(path)
                children.setdefault(self.parent[path], []).append(path)
            for index, item in enumerate(self.files[path]["items"]):
                node = (path, index, None)
                names = set(item.get("names", ()))
                if item["kind"] == "macro_call":
                    names |= expands.get(item["macro"], set())
                names_of[node] = names
                if item["kind"] == "glue":
                    seeds.append(node)
                elif item["kind"] == "macro_call" and not names:
                    for name in item["mentions"]:
                        by_type.setdefault(name, []).append(node)
                elif item["kind"] == "impl":
                    required = {name for own, foreign in self.requirement(path, item) for name in own + foreign}
                    members = [(path, index, member) for member in range(len(item["members"]))] if item["inherent"] else []
                    for member_node in members:
                        name = item["members"][member_node[2]]["name"]
                        names_of[member_node] = set()
                        if name:
                            by_name.setdefault(name, []).append(member_node)
                    for name in required:
                        for other in [node] + members:
                            by_type.setdefault(name, []).append(other)
                    if not required:
                        seeds.append(node)
                        seeds += [member_node for member_node in members if not item["members"][member_node[2]]["name"]]
                else:
                    for name in names:
                        by_name.setdefault(name, []).append(node)
                        by_file_name.setdefault((path, name), []).append(node)
        self.tables = dict(idents=idents, sees=sees, seen_by=seen_by, names_of=names_of, by_name=by_name,
                           by_file_name=by_file_name, by_type=by_type, by_module=by_module, children=children,
                           seeds=seeds, node_sets={})
        return self.tables

    def reachable(self, entries, universe, reasons=None, stop=frozenset()):
        crates = frozenset(crate_of(path) for path in universe)
        key = (frozenset(entries), crates)
        if reasons is None and key in self.results:
            return self.results[key] & set(universe)
        everything = {path for path in self.files if crate_of(path) in crates} | set(universe)
        files, complete = self.search(entries, everything, reasons, stop)
        if complete:
            self.results[key] = files
        return files & set(universe)

    def search(self, entries, universe, reasons, stop=frozenset()):
        targets = {self.target(path) for path in entries}
        universe = {path for path in universe if (self.is_library(path) or self.target(path) in targets)
                    and path not in self.test_only}
        tables = self.index()
        idents, sees, seen_by, names_of = tables["idents"], tables["sees"], tables["seen_by"], tables["names_of"]
        node_sets = tables["node_sets"]
        by_name, by_file_name, by_type = tables["by_name"], tables["by_file_name"], tables["by_type"]
        by_module, children = tables["by_module"], tables["children"]
        seeds = [node for node in tables["seeds"] if node[0] in universe]
        for path in universe:
            if (path in entries or (path not in self.parent and path not in self.roots)) and self.parse(path):
                seeds += [(path, index, None) for index in range(len(self.parse(path)["items"]))]

        def names(node):
            return names_of.get(node, set())

        def sets_of(node, item):
            if node not in node_sets:
                source = item["members"][node[2]] if node[2] is not None else item
                node_sets[node] = {key: set(source.get(key, ())) for key in ("mentions", "paths", "qualified", "invokes")}
            return node_sets[node]

        def item_of(node):
            return self.parse(node[0])["items"][node[1]]

        reached, opened, globbed, files, queue = set(), set(), set(), set(), []
        mentioned, qualified, invoked = {}, {}, {}
        done = {"mentions": {}, "qualified": {}, "invokes": {}, "paths": {}}
        path_names, types = {}, {}
        local = {}
        local_types = {}

        def heard(table, name, crate):
            sources = table.get(name)
            return bool(sources) and not sources.isdisjoint(seen_by.get(crate, {crate}))

        def ready(node):
            path, index, member = node
            item = item_of(node)
            crate = crate_of(path)
            if item["kind"] == "impl":
                visible = visible_types(crate)
                here = local_types.get(path, set())
                if not all((not own and not foreign) or here.intersection(own) or visible.intersection(foreign)
                           for own, foreign in self.requirement(path, item)):
                    return False
                name = item["members"][member]["name"] if member is not None else None
                return name is None or heard(mentioned, name, crate)
            if item["kind"] == "macro_call" and not names(node):
                return not visible_types(crate).isdisjoint(item["mentions"])
            names_ = names(node)
            if item["kind"] == "macro":
                return any(heard(invoked, name, crate) for name in names_)
            if item["kind"] == "use" and item["glob"]:
                return path in opened
            nearby = local.get(path, ())
            if any(name in nearby for name in names_):
                return True
            if item["kind"] == "use" and not item["public"]:
                return False
            if path in opened and any(heard(qualified, name, crate) for name in names_):
                return True
            return path in globbed and any(heard(mentioned, name, crate) for name in names_)

        visible_cache = {}

        def visible_types(crate):
            if crate not in visible_cache:
                visible_cache[crate] = set().union(*(types.get(other, set()) for other in sees.get(crate, {crate})))
            return visible_cache[crate]

        def wake(nodes, why):
            for other in nodes:
                if other not in reached and other[0] in universe and ready(other):
                    reach(other, why)

        def open_module(path, why):
            pending = [path]
            while pending:
                current = pending.pop()
                if current in opened:
                    continue
                opened.add(current)
                crate = crate_of(current)
                pending += [child for child in children.get(current, ())
                            if child in universe and self.module_name[child] in path_names.get(crate, ())]
                parsed = self.parse(current)
                wake([(current, index, None) for index, item in enumerate(parsed["items"] if parsed else ())
                      if item["kind"] != "impl"], f"{current} opened ({why})")

        def reach(node, why):
            if node in reached:
                return
            reached.add(node)
            queue.append(node)
            path, _, member = node
            item = item_of(node)
            if (item["kind"] == "use" and not item["public"]) or (item["kind"] == "impl" and item["inherent"] and member is None):
                return
            if path not in files:
                files.add(path)
                if reasons is not None:
                    reasons[path] = why
                if path in stop:
                    raise Stop

        try:
            for path in universe:
                if path in entries or path not in self.parent and not path.endswith("/src/lib.rs"):
                    open_module(path, "entry, or a file no module places")
            for node in seeds:
                reach(node, "entry" if node[0] in entries else "always counted")
            for path in entries & universe:
                files.add(path)
                if reasons is not None:
                    reasons.setdefault(path, "entry")
            while queue:
                node = queue.pop()
                path, index, member = node
                item = item_of(node)
                crate = crate_of(path)
                label = (item.get("names") or [item["kind"]])[0] if member is None else item["members"][member]["name"]
                source = sets_of(node, item)
                why = f"{path} ({label})"
                for word in (source["mentions"] | source["paths"]) & idents.keys():
                    root = f"crates/{idents[word]}/src/lib.rs"
                    if root in universe and root not in opened:
                        open_module(root, f"{why} names {word}")
                ancestor, new = path, source["mentions"]
                while ancestor is not None and new:
                    known = local.setdefault(ancestor, set())
                    new = new - known
                    known |= new
                    for name in new:
                        wake(by_file_name.get((ancestor, name), ()), f"{why} mentions {name}")
                    ancestor = self.parent.get(ancestor)
                if item["kind"] == "use" and item["glob"]:
                    for name in item["names"]:
                        for target in sees.get(crate, {crate}):
                            for module in list(by_module.get((target, name), ())) + \
                                    ([f"crates/{idents[name]}/src/lib.rs"] if name in idents else []):
                                if module not in globbed and module in universe:
                                    globbed.add(module)
                                    wake([(module, index, None) for index in range(len(self.parse(module)["items"]))]
                                         if self.parse(module) else [], f"{why} glob-imports {name}")
                visible_to = sees.get(crate, {crate})
                new = source["paths"] - done["paths"].setdefault(crate, set())
                done["paths"][crate] |= new
                for name in new:
                    for target in visible_to:
                        path_names.setdefault(target, set()).add(name)
                        for module in by_module.get((target, name), ()):
                            if module in universe and self.parent[module] in opened:
                                open_module(module, f"{why} names {name}")
                for key, table, wanted in (("qualified", qualified, lambda other: True),
                                           ("invokes", invoked, lambda other: item_of(other)["kind"] == "macro"),
                                           ("mentions", mentioned, lambda other: other[2] is not None or other[0] in globbed)):
                    new = source[key] - done[key].setdefault(crate, set())
                    done[key][crate] |= new
                    for name in new:
                        table.setdefault(name, set()).add(crate)
                        wake([other for other in by_name.get(name, ()) if crate_of(other[0]) in visible_to and wanted(other)],
                             f"{why} {key} {name}")
                if item["kind"] in TYPE_KINDS and member is None:
                    own = types.setdefault(crate, set())
                    local_types.setdefault(path, set()).update(names(node))
                    if not names(node) <= own:
                        own |= names(node)
                        visible_cache.clear()
                    seeing = seen_by.get(crate, {crate})
                    for name in names(node):
                        wake([other for other in by_type.get(name, ()) if crate_of(other[0]) in seeing],
                             f"implements for {name}, defined in {path}")
        except Stop:
            return files, False
        return files, True


class Stop(Exception):
    pass


def impl_subject(header):
    header, parameters = header.strip(), set()
    if header.startswith("<"):
        depth = 0
        for index, character in enumerate(header):
            depth += {"<": 1, ">": -1}.get(character, 0)
            if depth == 0:
                generics, header = header[1:index], header[index + 1:]
                parameters = {match.group(1) for match in re.finditer(rf"(?:^|,)\s*(?:const\s+)?({IDENT})", generics)}
                break
    return parameters, re.split(r"\bwhere\b", header)[0]


_CRATES = {}


def crate_of(path):
    crate = _CRATES.get(path)
    if crate is None:
        crate = _CRATES[path] = path.split("/", 2)[1]
    return crate


def is_workspace_rust(path):
    return path.startswith("crates/") and path.endswith(".rs") and path.count("/") >= 2


PATH_DEPENDENCY = re.compile(r'path\s*=\s*"\.\./([\w-]+)"')


@functools.cache
def _path_dependencies(root, crate):
    manifest = Path(root) / "crates" / crate / "Cargo.toml"
    text = manifest.read_text(encoding="utf-8") if manifest.is_file() else ""
    return frozenset(PATH_DEPENDENCY.findall(text))


def linked_crates(root, crates):
    found, queue = set(crates), list(crates)
    while queue:
        for dependency in _path_dependencies(str(root), queue.pop()):
            if dependency not in found:
                found.add(dependency)
                queue.append(dependency)
    return found


def unrecorded(root, plain, changed):
    crates = {crate_of(path) for path in plain if path.startswith("crates/") and path.count("/") >= 2}
    linked = linked_crates(root, crates) - crates
    return {path for path in changed if is_workspace_rust(path) and crate_of(path) in linked}


DECLARATION_LINE = re.compile(rf"\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+({IDENT})\s*;")


def declarations(text, names, keep_rest):
    lines, found, attributes = text.splitlines(), [], []
    for line in lines:
        if line.strip().startswith("#[") and not line.strip().startswith("#!["):
            attributes.append(line.strip())
            continue
        match = DECLARATION_LINE.match(line)
        if match and match.group(1) in names:
            found += attributes + [line.strip()]
        elif keep_rest and not match:
            found += attributes + [line]
        attributes = []
    return "\n".join(found + attributes)


def declarations_changed(root, path, children, base, whole):
    if base is None:
        return True
    return _declarations_changed(str(root), path, frozenset(children), base, whole)


@functools.lru_cache(maxsize=None)
def _declarations_changed(root, path, children, base, whole):
    try:
        head = (Path(root) / path).read_text(encoding="utf-8", errors="replace")
        before = subprocess.run(["git", "show", f"{base}:{path}"], cwd=root, capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return True
    return declarations(head, children, whole) != declarations(before, children, whole)


def narrow(dependencies, artifacts, graph, changed=(), stop=frozenset()):
    plain = [dependency.split("#keep:")[0] for dependency in dependencies]
    crates = {crate_of(path) for path in plain if path.startswith("crates/") and path.count("/") >= 2}
    rust = {path for path in plain if is_workspace_rust(path)}
    added = {path for path in changed if is_workspace_rust(path) and crate_of(path) in linked_crates(graph.root, crates)
             and path not in rust and (graph.root / path).is_file()}
    entries = graph.entries(artifacts, rust)
    if entries is None:
        return {dependency: (True, None) for dependency in dependencies}
    missing = {path for path in rust if graph.parse(path) is None}
    reached = graph.reachable(entries, rust | added | entries, stop=frozenset(stop) & (rust | added)) | missing
    named, named_in_crate = set(), {}
    for path in rust | added:
        parsed = graph.parse(path)
        file_names = set(parsed["file_names"]) if parsed else set()
        if path in reached:
            named |= file_names
        named_in_crate.setdefault(crate_of(path), set()).update(file_names)
    kept = {}
    for dependency, path in zip(dependencies, plain):
        if is_workspace_rust(path):
            if path in reached:
                kept[dependency] = (True, set())
        elif not path.startswith("crates/") or path.endswith(("Cargo.toml", "build.rs")) \
                or path.rsplit("/", 1)[-1] in named \
                or path.rsplit("/", 1)[-1] not in named_in_crate.get(crate_of(path), ()):
            kept[dependency] = (True, None)
    for path in sorted(added & reached):
        kept[path] = (True, set())
    for path in sorted(reached):
        child = path
        while child in graph.parent:
            parent = graph.parent[child]
            kept.setdefault(parent, (False, set()))[1].add(graph.module_name[child])
            child = parent
    return kept
