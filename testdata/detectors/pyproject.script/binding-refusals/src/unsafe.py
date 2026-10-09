from pathlib import Path
Path('EXECUTED').write_text('forbidden')
exec(payload)
def main(): pass
