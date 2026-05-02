#!/usr/bin/env python3
"""
Script to generate LaTeX table rows from benchmark results CSV.
Columns: U, s, constraints, Proof_size, Proof time (avg), verify time (avg)
"""

import pandas as pd
import sys

def generate_latex_rows(csv_file):
    """
    Read CSV file and generate LaTeX table rows.
    
    Args:
        csv_file (str): Path to the CSV file
    
    Returns:
        str: LaTeX formatted table rows
    """
    # Read the CSV file
    df = pd.read_csv(csv_file)
    
    # Remove any empty rows
    df = df.dropna(subset=['U'])
    
    # Sort by U column
    df = df.sort_values('U')
    
    # Constants for calculation
    Ra = 128
    F0 = 8000
    
    latex_rows = []
    
    for _, row in df.iterrows():
        U = int(row['U'])
        s = U * Ra / F0  # Calculate s = U*Ra/F0
        constraints = int(row['constraints'])
        proof_size = row['proof_size']  # in bytes
        proof_time_avg = row['prover_time_avg_ms'] / 1000  # convert ms to seconds
        verify_time_avg = row['verify_time_avg_ms'] / 1000  # convert ms to seconds
        
        # Format the LaTeX row
        latex_row = f"        {U:2d}   & {s:4.1f}  & {constraints:,}  & {proof_size:.0f}  & {proof_time_avg:.2f}  & {verify_time_avg:.3f} \\\\"
        latex_rows.append(latex_row)
    
    return '\n'.join(latex_rows)

def main():
    csv_file = 'benchmark_results.csv'
    
    try:
        latex_output = generate_latex_rows(csv_file)
        print("LaTeX table rows:")
        print(latex_output)
        
        # Also save to a file
        with open('latex_output.txt', 'w') as f:
            f.write(latex_output)
        print(f"\nOutput also saved to latex_output.txt")
        
    except FileNotFoundError:
        print(f"Error: Could not find {csv_file}")
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
