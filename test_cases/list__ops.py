# === List concatenation (+) ===
assert [1, 2] + [3, 4] == [1, 2, 3, 4], 'basic concat'
assert [] + [1, 2] == [1, 2], 'empty left concat'
assert [1, 2] + [] == [1, 2], 'empty right concat'
assert [] + [] == [], 'empty both concat'
assert [1] + [2] + [3] + [4] == [1, 2, 3, 4], 'multiple concat'
assert [[1]] + [[2]] == [[1], [2]], 'nested concat'

# === Augmented assignment (+=) ===
lst = [1, 2]
lst += [3, 4]
assert lst == [1, 2, 3, 4], 'basic iadd'

lst = [1]
lst += []
assert lst == [1], 'iadd empty'

lst = [1]
lst += [2]
lst += [3]
assert lst == [1, 2, 3], 'multiple iadd'

lst = [1, 2]
lst += lst
assert lst == [1, 2, 1, 2], 'iadd self'

# === List length ===
assert len([]) == 0, 'len empty'
assert len([1, 2, 3]) == 3, 'len basic'

lst = [1]
lst.append(2)
assert len(lst) == 2, 'len after append'

# === List indexing ===
a = []
a.append('value')
assert a[0] == 'value', 'getitem basic'

a = [1, 2, 3]
assert a[0 - 1] == 3, 'getitem negative index'
assert a[-1] == 3, 'getitem -1'
assert a[-2] == 2, 'getitem -2'

# === List repr/str ===
assert repr([]) == '[]', 'empty list repr'
assert str([]) == '[]', 'empty list str'

assert repr([1, 2, 3]) == '[1, 2, 3]', 'list repr'
assert str([1, 2, 3]) == '[1, 2, 3]', 'list str'

# === List repetition (*) ===
assert [1, 2] * 3 == [1, 2, 1, 2, 1, 2], 'list mult int'
assert 3 * [1, 2] == [1, 2, 1, 2, 1, 2], 'int mult list'
assert [1] * 0 == [], 'list mult zero'
assert [1] * -1 == [], 'list mult negative'
assert [] * 5 == [], 'empty list mult'
assert [1, 2] * 1 == [1, 2], 'list mult one'
assert [[1]] * 2 == [[1], [1]], 'nested list mult'

# === List repetition augmented assignment (*=) ===
lst = [1, 2]
lst *= 2
assert lst == [1, 2, 1, 2], 'list imult'

lst = [1]
lst *= 0
assert lst == [], 'list imult zero'
