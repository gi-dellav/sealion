def obj_props(n):
    o = {"a": 0, "b": 0, "c": 0, "d": 0}
    keys = ["a", "b", "c", "d"]
    acc = 0
    for i in range(n):
        o["a"] = o["a"] + 1
        o["b"] = o["b"] + 2
        o["c"] = o["c"] + 3
        k = keys[i & 3]
        o[k] = o[k] + 4
        acc = (acc + o[k] + o["c"]) % 65536
    return acc


if __name__ == "__main__":
    print(obj_props(300_000) % 256)
