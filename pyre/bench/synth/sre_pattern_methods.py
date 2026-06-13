import re

N = 30000

pair = re.compile(r"([a-z]+)(\d+)")
digit = re.compile(r"\d")


def main():
    subject = "ab12 cd34 ef56"
    acc = 0
    i = 0
    while i < N:
        # findall with groups -> list of tuples
        for a, b in pair.findall(subject):
            acc = acc + len(a) + len(b)
        # finditer -> scanner yielding Match objects
        for m in pair.finditer(subject):
            acc = acc + m.start() + m.end()
            acc = acc + len(m.expand(r"\2\1"))
        # sub with a backreference template
        acc = acc + len(pair.sub(r"\2\1", subject))
        # sub with a callable replacement
        acc = acc + len(digit.sub(lambda mo: "[" + mo.group() + "]", subject))
        # subn returns (string, count)
        _, n = digit.subn("#", subject)
        acc = acc + n
        i = i + 1
    print(acc)


main()
