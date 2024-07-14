from typing import Tuple
from io import StringIO
import sys

import duckdb


def render_df_info(duckdf: duckdb.DuckDBPyRelation) -> str:
    """ returns md like formatted df.info """
    shape = duckdf.shape

    output_buffer = StringIO()
    sys.stdout = output_buffer
    duckdf.describe().show(max_width=10**19)
    sys.stdout = sys.__stdout__
    descr = output_buffer.getvalue()

    h = f"### Rows: {shape[0]}, Columns: {shape[1]}\n{'-'*50}\n"
    try:
        lines = descr.strip().split("\n")
        headers = lines[1].strip("│").split("│")
        headers = [header.strip() for header in headers]
        types = lines[2].strip("│").split("│")
        types = [t.strip() for t in types]
        data_lines = lines[4:-1]
        data = []
        for line in data_lines:
            row = line.strip("│").split("│")
            row = [item.strip() for item in row]
            data.append(row)

        # Build the markdown table
        markdown_table = "| " + " | ".join(headers) + " |\n"
        markdown_table += "|-" + "-|-".join(["-" * len(header) for header in headers]) + "-|\n"
        markdown_table += "| " + " | ".join(types) + " |\n"
        for row in data:
            markdown_table += "| " + " | ".join(row) + " |\n"

        return h + markdown_table
    
    except Exception as e:
        return h + '\n' + descr
