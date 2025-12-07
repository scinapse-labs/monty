# skip=cpython
# invalid format specifier with trailing characters (detected at parse time)
f'{1:10xyz}'
# ParseError=AST: Invalid format specifier '10xyz'
