# skip=cpython
# invalid format specifier for string (detected at parse time)
f'{"hello":abc}'
# ParseError=AST: Invalid format specifier 'abc'
