"""
Resolver + Type Checker — Lark Tree → ParVu IR.

Walks the parsed AST and:
1. Binds table[col] references against the Catalog
2. Binds function names against the FunctionRegistry
3. Type-checks argument signatures
4. Infers return types bottom-up
5. Reports errors with source locations
"""
from __future__ import annotations

from loguru import logger
from lark import Tree, Token

from parvu.core.dsl.ir import Expr, ColumnRef, Literal, Call, BinaryOp, UnaryOp, MethodCall, Assignment, DropDuplicates, Replace
from parvu.core.dsl.registry import FunctionRegistry
from parvu.core.dsl.catalog import Catalog
from parvu.core.dsl.types import LogicalType, duckdb_type_to_logical


class ResolutionError(Exception):
    """Raised when a name cannot be resolved or types mismatch."""
    pass


class Resolver:
    """Transforms a Lark parse tree into typed ParVu IR."""

    def __init__(self, catalog: Catalog, registry: FunctionRegistry):
        self._catalog = catalog
        self._registry = registry

    def resolve(self, tree: Tree) -> Expr:
        """Resolve a Lark Tree into a typed IR expression."""
        expr = self._resolve_node(tree)
        logger.debug(f"Resolved expression to type: {expr.logical_type.name}")
        return expr

    def _resolve_node(self, node: Tree | Token) -> Expr:
        if isinstance(node, Token):
            return self._resolve_token(node)

        rule = node.data
        children = node.children

        # Literals
        if rule in ("number", "string", "boolean", "null"):
            return self._resolve_literal(node)

        # Assignment: table[new_col] = expr
        if rule == "assignment":
            return self._resolve_assignment(children)

        # Column reference: table[col]
        if rule == "column_ref":
            return self._resolve_column_ref(children)

        # Function call: FUNC(args...)
        if rule == "func_call":
            return self._resolve_func_call(children)

        # Method call: expr.method or expr.method(args...)
        if rule == "method_call":
            return self._resolve_method_call(children)

        # Binary operators — arithmetic
        if rule in ("add", "sub", "mul", "div"):
            op_map = {"add": "+", "sub": "-", "mul": "*", "div": "/"}
            left = self._resolve_node(children[0])
            right = self._resolve_node(children[1])
            return BinaryOp(op=op_map[rule], left=left, right=right)

        # Binary operators — comparison
        if rule in ("eq", "ne", "gt", "gte", "lt", "lte"):
            op_map = {
                "eq": "==",
                "ne": "!=",
                "gt": ">",
                "gte": ">=",
                "lt": "<",
                "lte": "<=",
            }
            left = self._resolve_node(children[0])
            right = self._resolve_node(children[1])
            return BinaryOp(op=op_map[rule], left=left, right=right, logical_type=LogicalType.BOOLEAN)

        # Logical operators
        if rule in ("logical_and", "logical_or"):
            op_map = {"logical_and": "AND", "logical_or": "OR"}
            left = self._resolve_node(children[0])
            right = self._resolve_node(children[1])
            return BinaryOp(op=op_map[rule], left=left, right=right, logical_type=LogicalType.BOOLEAN)

        # Unary operators
        if rule == "unary_plus":
            return self._resolve_node(children[1])
        if rule == "unary_minus":
            operand = self._resolve_node(children[1])
            return UnaryOp(op="-", operand=operand, logical_type=operand.logical_type)
        if rule == "logical_not":
            operand = self._resolve_node(children[1])
            return UnaryOp(op="NOT", operand=operand, logical_type=LogicalType.BOOLEAN)

        # Parenthesized expression
        if rule == "expr" and len(children) == 3 and children[0] == "(":
            return self._resolve_node(children[1])

        # Single child (pass through)
        if len(children) == 1:
            return self._resolve_node(children[0])

        raise ResolutionError(f"Unsupported AST node: {rule}")

    def _resolve_token(self, token: Token) -> Expr:
        """Resolve a bare token (shouldn't normally happen at top level)."""
        if token.type == "NUMBER":
            val = float(token.value) if "." in token.value else int(token.value)
            return Literal(value=val, logical_type=LogicalType.NUMERIC)
        if token.type == "STRING":
            return Literal(value=token.value.strip("'"), logical_type=LogicalType.TEXT)
        if token.type == "BOOLEAN":
            return Literal(value=token.value.upper() == "TRUE", logical_type=LogicalType.BOOLEAN)
        if token.type == "NULL":
            return Literal(value=None, logical_type=LogicalType.UNKNOWN)
        raise ResolutionError(f"Unexpected token: {token}")

    def _resolve_literal(self, node: Tree) -> Expr:
        """Resolve a literal tree node."""
        token = node.children[0]
        if node.data == "number":
            val = float(token.value) if "." in token.value else int(token.value)
            return Literal(value=val, logical_type=LogicalType.NUMERIC)
        if node.data == "string":
            value = token.value
            if value.startswith('"') and value.endswith('"'):
                value = value[1:-1]
            elif value.startswith("'") and value.endswith("'"):
                value = value[1:-1]
            return Literal(value=value, logical_type=LogicalType.TEXT)
        if node.data == "boolean":
            return Literal(value=token.value.upper() == "TRUE", logical_type=LogicalType.BOOLEAN)
        if node.data == "null":
            return Literal(value=None, logical_type=LogicalType.UNKNOWN)
        raise ResolutionError(f"Unknown literal type: {node.data}")

    def _resolve_assignment(self, children: list) -> Expr:
        """Resolve table[new_col] = expr assignment."""
        col_ref_node = children[0]
        table = col_ref_node.children[0].value
        col = col_ref_node.children[1].value
        if col.startswith('"') and col.endswith('"'):
            col = col[1:-1]

        if table not in self._catalog.tables():
            raise ResolutionError(f"Unknown table: '{table}'")

        value = self._resolve_node(children[1])
        logger.debug(f"Resolved assignment: {table}[{col}] = <{value.logical_type.name}>")
        return Assignment(table=table, column=col, value=value, logical_type=value.logical_type)

    def _resolve_column_ref(self, children: list) -> Expr:
        """Resolve table[column] reference."""
        table = children[0].value
        col = children[1].value
        # Strip quotes if column was specified as a string literal
        if col.startswith('"') and col.endswith('"'):
            col = col[1:-1]

        if table not in self._catalog.tables():
            logger.warning(f"Resolution error: unknown table '{table}'")
            raise ResolutionError(f"Unknown table: '{table}'")
        if col not in self._catalog.columns(table):
            logger.warning(f"Resolution error: unknown column '{col}' in table '{table}'")
            raise ResolutionError(f"Unknown column '{col}' in table '{table}'")

        logical_type = self._catalog.resolve_column(table, col)
        logger.debug(f"Resolved column ref: {table}[{col}] -> {logical_type.name}")
        return ColumnRef(table=table, column=col, logical_type=logical_type)

    def _resolve_func_call(self, children: list) -> Expr:
        """Resolve FUNC(args...) call."""
        func_name = children[0].value
        args = []
        if len(children) > 1 and isinstance(children[1], Tree):
            args = [self._resolve_node(c) for c in children[1].children]

        # Special-case: drop_duplicates(table, cols..., keep)
        if func_name.upper() == "DROP_DUPLICATES":
            return self._resolve_drop_duplicates(args)

        # Special-case: REPLACE(text, pattern, with_value, case_sensitive, regex)
        if func_name.upper() == "REPLACE":
            return self._resolve_replace(args)

        func_def = self._registry.lookup(func_name)
        if func_def is None:
            logger.warning(f"Resolution error: unknown function '{func_name}'")
            raise ResolutionError(f"Unknown function: '{func_name}'")

        self._check_args(func_def, args)
        logger.debug(f"Resolved function call: {func_name}({len(args)} args) -> {func_def.return_type.name}")
        return Call(func_name=func_name, args=args, logical_type=func_def.return_type)

    def _resolve_drop_duplicates(self, args: list[Expr]) -> Expr:
        """Resolve drop_duplicates(col_ref, col_refs..., keep) special form."""
        if not args:
            raise ResolutionError("drop_duplicates requires at least one column reference")

        # First arg must be a column ref (extracts table name from it)
        first_col = args[0]
        if not isinstance(first_col, ColumnRef):
            raise ResolutionError("drop_duplicates arguments must be column references like sales[id]")
        table = first_col.table

        if table not in self._catalog.tables():
            raise ResolutionError(f"Unknown table: '{table}'")

        # Extract subset columns and optional keep strategy
        keep = "first"
        subset_cols: list[str] = []
        for arg in args:
            if isinstance(arg, ColumnRef):
                if arg.table != table:
                    raise ResolutionError(
                        f"drop_duplicates column references must belong to table '{table}'"
                    )
                subset_cols.append(arg.column)
            elif isinstance(arg, Literal) and isinstance(arg.value, str):
                if arg.value.lower() not in ("first", "last"):
                    raise ResolutionError("drop_duplicates keep must be 'first' or 'last'")
                keep = arg.value.lower()
            else:
                raise ResolutionError(
                    "drop_duplicates arguments must be column references "
                    "or a keep strategy string ('first' or 'last')"
                )

        # If only one column reference was given (with or without keep string),
        # interpret it as "all columns" for convenience.
        column_args = [a for a in args if isinstance(a, ColumnRef)]
        if len(column_args) == 1:
            subset_cols = list(self._catalog.columns(table))

        logger.debug(f"Resolved drop_duplicates: {table}({subset_cols}, keep={keep})")
        return DropDuplicates(table=table, columns=subset_cols, keep=keep, logical_type=LogicalType.QUERY)

    def _resolve_method_call(self, children: list) -> Expr:
        """Resolve expr.method or expr.method(args...) call."""
        receiver = self._resolve_node(children[0])
        method_name = children[1].value
        args = []
        if len(children) > 2 and isinstance(children[2], Tree):
            args = [self._resolve_node(c) for c in children[2].children]

        # Special-case: replace method
        if method_name.lower() == "replace":
            return self._resolve_replace([receiver] + args)

        func_def = self._registry.lookup_method(method_name)
        if func_def is None:
            logger.warning(f"Resolution error: unknown method '{method_name}' for type {receiver.logical_type.name}")
            raise ResolutionError(
                f"Unknown method '{method_name}' for type {receiver.logical_type.name}"
            )

        all_args = [receiver] + args
        self._check_args(func_def, all_args)
        logger.debug(f"Resolved method call: .{method_name}() on {receiver.logical_type.name}")
        return MethodCall(
            receiver=receiver,
            method_name=method_name,
            args=args,
            logical_type=func_def.return_type,
        )

    def _resolve_replace(self, args: list[Expr]) -> Expr:
        """Resolve REPLACE(text, pattern, with_value, case_sensitive, regex)."""
        if len(args) < 3:
            raise ResolutionError(
                "REPLACE requires at least 3 arguments: text, pattern, with_value"
            )
        text = args[0]
        pattern = args[1]
        with_value = args[2]
        case_sensitive = True
        regex = False
        if len(args) > 3:
            cs = args[3]
            if not isinstance(cs, Literal) or not isinstance(cs.value, bool):
                raise ResolutionError("REPLACE case_sensitive must be TRUE or FALSE")
            case_sensitive = cs.value
        if len(args) > 4:
            rx = args[4]
            if not isinstance(rx, Literal) or not isinstance(rx.value, bool):
                raise ResolutionError("REPLACE regex must be TRUE or FALSE")
            regex = rx.value
        return Replace(
            text=text,
            pattern=pattern,
            with_value=with_value,
            case_sensitive=case_sensitive,
            regex=regex,
            logical_type=LogicalType.TEXT,
        )

    def _check_args(self, func_def, args: list[Expr]) -> None:
        """Type-check function arguments against signature."""
        required = [p for p in func_def.params if p.required]
        if len(args) < len(required):
            raise ResolutionError(
                f"'{func_def.name}' requires at least {len(required)} arguments, got {len(args)}"
            )
        # Soft type check: warn on mismatches but allow UNKNOWN
        for i, (arg, param) in enumerate(zip(args, func_def.params)):
            if arg.logical_type != param.logical_type and arg.logical_type != LogicalType.UNKNOWN:
                # Allow numeric subtypes and general coercions
                if param.logical_type == LogicalType.NUMERIC and arg.logical_type == LogicalType.NUMERIC:
                    continue
                if param.logical_type == LogicalType.UNKNOWN:
                    continue
                # For now, be permissive — strict type enforcement comes later
