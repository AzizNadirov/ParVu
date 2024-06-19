from io import StringIO
import pandas as pd
from pathlib import Path



def render_df_info(table_path: Path) -> str:
    """ returns md like formatted df.info """
    buff = StringIO()
    df = pd.read_parquet(table_path)
    df.info(buf=buff)
    info_str = buff.getvalue()
    lines = info_str.split("\n")
    shape = f"###  Rows{df.shape[0]}, Columns: {df.shape[1]}"
    column_lines = lines[5:27]
    markdown_table = "| # | Column | Non-Null Count | Dtype |\n"
    markdown_table += "|---|--------|----------------|-------|\n"

    # Iterate over the column lines and extract the details
    for line in column_lines:
        parts = line.split()
        index = parts[0]
        column = " ".join(parts[1:-3])
        non_null_count = parts[-3]
        dtype = parts[-1]
        
        # Add the row to the markdown table
        markdown_table += f"| {index} | {column} | {non_null_count} | {dtype} |\n"

    info = f"{shape}\n{markdown_table}" + \
    """\n\n**tip: you can run SQL query `SHOW <table_name>` or `DESCRIBE <table_name>` to get more info.
        However, running running table this way more expensive in terms of memory.**\n\n\t`Esc` to exit...
    """

    return info