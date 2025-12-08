# === Bytes length ===
assert len(b'') == 0, 'len empty'
assert len(b'hello') == 5, 'len basic'

# === Bytes repr/str ===
assert repr(b'hello') == "b'hello'", 'bytes repr'
assert str(b'hello') == "b'hello'", 'bytes str'

# === Various bytes repr cases ===
assert repr(b'') == "b''", 'empty bytes repr'
assert repr(b"it's") == 'b"it\'s"', 'single quote bytes repr'
assert repr(b'l1\nl2') == "b'l1\\nl2'", 'newline bytes repr'
assert repr(b'col1\tcol2') == "b'col1\\tcol2'", 'tab bytes repr'
assert repr(b'\x00\xff') == "b'\\x00\\xff'", 'non-printable bytes repr'
assert repr(b'back\\slash') == "b'back\\\\slash'", 'backslash bytes repr'

# === Bytes repetition (*) ===
assert b'ab' * 3 == b'ababab', 'bytes mult int'
assert 3 * b'ab' == b'ababab', 'int mult bytes'
assert b'x' * 0 == b'', 'bytes mult zero'
assert b'x' * -1 == b'', 'bytes mult negative'
assert b'' * 5 == b'', 'empty bytes mult'
assert b'ab' * 1 == b'ab', 'bytes mult one'
