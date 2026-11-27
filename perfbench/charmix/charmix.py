def char_mix(passes, n):
    src = ""
    for i in range(n):
        src = src + chr(65 + (i % 26))
    acc = 0
    for _ in range(passes):
        for i in range(n):
            acc = (acc + ord(src[i]) * ((i % 7) + 1)) % 65536
    return acc


if __name__ == "__main__":
    print(char_mix(1000, 1024) % 256)
