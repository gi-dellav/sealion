def str_concat(rounds, pieces):
    acc = 0
    for r in range(rounds):
        s = ""
        for _ in range(pieces):
            s = s + "abcdefgh"
        acc = (acc + len(s) + ord(s[r])) % 65536
    return acc


if __name__ == "__main__":
    print(str_concat(200, 2000) % 256)
