import json
import click
import subprocess
import pathlib
import os
import shutil

from pybrary_extraction.python2lisp import Py2Lisp
from pybrary_extraction.lisp2python import Lisp2Py, Abstraction2Py, Rewrite2Py


def try_make_parent_dir(new_file_path):
    try:
        os.makedirs(pathlib.Path(new_file_path).parent)
    except FileExistsError:
        pass


class Leroy:
    def __init__(self, py_files_dir, iterations,
                 max_arity, min_nodes_abstraction):

        self.py_files_dir = py_files_dir
        self.min_nodes_abstraction = min_nodes_abstraction
        self.iterations = iterations
        self.max_arity = max_arity
        self.project_base_dir = pathlib.Path(__file__).parent.parent
        self.temp_dir = self.project_base_dir.joinpath("temp")
        self.temp_filename = "in.json"
        self.stitch_outfile = "out.json"

        # file path -> lisp-encoded ast
        self.file_json_map = None
        self.stitch_out = self.read_stitch_out()

    def run(self):

        self.clear_temp_dir()
        self.file_json_map = Py2Lisp.fromDirectoryToJson(self.py_files_dir)
        with open(f"{self.temp_dir}/{self.temp_filename}", "w") as f:
            json.dump(list(self.file_json_map.values()), f, indent=4)

        if not self.result_is_cached():
            self.run_stitch()
        self.write_files()

    def run_stitch(self):
        subprocess.run(
            [
                "cargo", "run",
                "--manifest-path", f"{self.project_base_dir}/Cargo.toml",
                "--release", '--bin=compress',
                '--', f'{self.temp_dir}/{self.temp_filename}',
                f'--iterations={self.iterations}',
                f'--max-arity={self.max_arity}',
                f'--out={self.temp_dir}/{self.stitch_outfile}',
                "--prune-macro-abstractions",
                "--no-opt-arity-zero",
                f"--min-nodes-invention={self.min_nodes_abstraction}"
            ]
        )

        self.stitch_out = self.read_stitch_out()

    def write_files(self):

        stitch_out = self.stitch_out

        stitch_rewritten = stitch_out['rewritten']
        stitch_abstractions = [i['body'] for i in stitch_out["abstractions"]]

        self.write_abstractions(stitch_abstractions)
        self.write_rewritten_programs(stitch_rewritten)

    def read_stitch_out(self):
        try:
            with open(f"{self.temp_dir}/{self.stitch_outfile}") as f:
                stitch_out = json.load(f)
            return stitch_out
        except FileNotFoundError:
            return

    def write_rewritten_programs(self, stitch_rewritten):
        for file, rewrite in zip(self.file_json_map.keys(), stitch_rewritten):
            new_file_path = file.replace(self.py_files_dir, str(self.temp_dir))
            try_make_parent_dir(new_file_path)
            print(f"{new_file_path=}")

            try:
                py_code = Rewrite2Py(rewrite).convert()
            except:
                print(f"Failed to rewrite: {rewrite}")
                raise

            print(py_code)
            with open(new_file_path, "w") as f:
                f.write(py_code)

    def write_abstractions(self, stitch_abstractions):
        library_functions = []
        for i, abs_body in enumerate(stitch_abstractions):
            library_functions.append(
                Abstraction2Py(abs_body).convert(f"fn_{i}")
            )
        with open(f"{self.temp_dir}/leroy_library.py", "w") as f:
            f.write("\n\n".join(library_functions))

    def clear_temp_dir(self):
        try:
            shutil.rmtree(self.temp_dir)
        except FileNotFoundError:
            pass
        os.makedirs(self.temp_dir)
        if self.stitch_out:
            with open(f"{self.temp_dir}/{self.stitch_outfile}", "w") as f:
                json.dump(self.stitch_out, f, indent=4)

    def result_is_cached(self):
        if self.stitch_out:
            replace_equivalent = \
                ['ProgramStatements' if i == '(ProgramStatements )' else i for i in list(self.file_json_map.values())]
            if set(self.stitch_out['original']) == set(replace_equivalent):
                return True
        return False


@click.command()
@click.option("--py_files_dir", help='directory containing python files to run leroy on.')
@click.option("--iterations", help='Number of iterations to run stitch for.', default=3, type=int)
@click.option("--max-arity", default=3, type=int, help='maximum number of parameters for an abstraction.')
@click.option("--min-nodes-abstraction", help='minimum number of ast nodes in the abstraction',
              default=15, type=int)
def run_leroy(py_files_dir, iterations, max_arity, min_nodes_abstraction):
    l = Leroy(py_files_dir, iterations, max_arity, min_nodes_abstraction)
    l.run()


if __name__ == '__main__':
    run_leroy()
