N = 900000


def main():
    i = 0
    acc = 0
    while i < N:
        t = [i, i + 1, i + 2, i + 3, i + 4, i + 5]
        acc = acc + t[0] + t[2] + t[5]
        i = i + 1
    print(acc)


main()
