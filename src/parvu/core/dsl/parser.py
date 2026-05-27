"""
DSL Parser — Lark-based grammar for the ParVu expression language.

Parses source text into a Lark Tree, which is then transformed into
the ParVu IR (Expr nodes) by the Resolver.
"""
from __future__ import annotations

from loguru import logger
from lark import Lark, Tree, Token
from lark.exceptions import LarkError

# EBNF grammar for the ParVu expression language
_GRAMMAR = r"""
?start: expr | assignment

assignment: column_ref "=" expr

?expr: or_expr

?or_expr: and_expr
        | or_expr "||" and_expr   -> logical_or
        | or_expr "OR" and_expr   -> logical_or

?and_expr: compare_expr
         | and_expr "&&" compare_expr  -> logical_and
         | and_expr "AND" compare_expr -> logical_and

?compare_expr: add_expr
             | compare_expr "==" add_expr -> eq
             | compare_expr "!=" add_expr -> ne
             | compare_expr "<>" add_expr -> ne
             | compare_expr ">" add_expr  -> gt
             | compare_expr ">=" add_expr -> gte
             | compare_expr "<" add_expr  -> lt
             | compare_expr "<=" add_expr -> lte

?add_expr: mul_expr
         | add_expr "+" mul_expr  -> add
         | add_expr "-" mul_expr  -> sub

?mul_expr: unary_expr
         | mul_expr "*" unary_expr  -> mul
         | mul_expr "/" unary_expr  -> div

?unary_expr: primary
           | "+" unary_expr   -> unary_plus
           | "-" unary_expr   -> unary_minus
           | "!" unary_expr   -> logical_not
           | "NOT" unary_expr -> logical_not

?primary: literal
        | column_ref
        | func_call
        | method_call
        | "(" expr ")"

func_call: IDENT "(" [arg_list] ")"
arg_list: expr ("," expr)*

column_ref: IDENT "[" (IDENT | STRING) "]"

method_call: primary "." IDENT ["(" [arg_list] ")"]

literal: NUMBER  -> number
       | STRING  -> string
       | BOOLEAN -> boolean
       | NULL    -> null

IDENT: /[a-zA-Z_][a-zA-Z0-9_]*/
NUMBER: /-?\d+(\.\d+)?/
STRING: /'[^']*'|"[^"]*"/
BOOLEAN.2: "TRUE" | "FALSE"
NULL.2: "NULL"

%import common.WS
%ignore WS
"""


class ParseError(Exception):
    """Raised when source text cannot be parsed."""
    pass


class DSLParser:
    """Parser for the ParVu expression language."""

    def __init__(self):
        self._parser = Lark(
            _GRAMMAR,
            parser="lalr",
            propagate_positions=True,
        )

    def parse(self, source: str) -> Tree:
        """Parse source text into a Lark Tree.

        Raises ParseError on failure.
        """
        try:
            tree = self._parser.parse(source)
            logger.debug(f"Parsed expression: {source[:40]}...")
            return tree
        except LarkError as e:
            logger.debug(f"Parse error for: {source[:40]}... | {e}")
            raise ParseError(str(e)) from e

    def parse_partial(self, source: str) -> Tree | None:
        """Attempt to parse, returning None on failure.

        Useful for autocomplete on incomplete input.
        """
        try:
            return self._parser.parse(source)
        except LarkError:
            return None
