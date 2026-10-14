import sys

from shop.service import place_order


def main():
    place_order(sys.argv[1])


if __name__ == "__main__":
    main()
