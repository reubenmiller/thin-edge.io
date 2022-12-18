"""Robot Framework debugger which can be used to
debug test libraries (not just the keywords/steps)

You can start this file from VSCode "Debug current file".
It will prompt you for the file to debug (if no arguments are provided).
"""
import sys
from pathlib import Path

import inquirer

import robot
from robot.model import SuiteVisitor
from robot.running import TestSuiteBuilder


class TestCasesFinder(SuiteVisitor):
    """Test case finder"""

    def __init__(self):
        self.tests = []

    def visit_test(self, test):
        self.tests.append(test)


def find_tests(test_path: str) -> TestCasesFinder:
    """Find tests in a given folder

    Args:
        test_path (str): Folder to search for tests in

    Returns:
        TestCasesFinder: Test case finder which contains
            a list of all of the tests that were found
    """
    builder = TestSuiteBuilder(rpa=False)
    testsuite = builder.build(test_path)
    finder = TestCasesFinder()
    testsuite.visit(finder)
    return finder


def main():
    """Main: Prompt the user for the tests to be run
    then execute it
    """
    options = {}

    # Get test dir (or explicit path to robot file)

    path = Path(__file__).parent.parent.resolve()
    if len(sys.argv) > 1:
        if sys.argv[1].lower().endswith(".robot"):
            path = sys.argv[1]

    # Find test files, and build a lookup list
    testcases = {item.longname: item for item in find_tests(path).tests}

    questions = [
        inquirer.List(
            "testcase",
            message="Which test case?",
            choices=list(testcases.keys()),
        ),
    ]

    answers = inquirer.prompt(questions)

    if not answers:
        sys.exit(0)

    testcase = answers.get("testcase", "")
    if testcase:
        options["test"] = testcase

    robot.run(str(path), **options)


if __name__ == "__main__":
    main()
