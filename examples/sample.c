#include <stdio.h>
#include <stdlib.h>

#define MAX_ITEMS 100

struct Item {
    char name[32];
    int price;
};

int factorial(int n) {
    if (n <= 1) {
        return 1;
    } else {
        return n * factorial(n - 1);
    }
}

int main(void) {
    struct Item items[MAX_ITEMS];
    int count = 0;
    int total = 0;
    int choice;

    printf("How many items? ");
    scanf("%d", &count);

    for (int i = 0; i < count; i++) {
        printf("Item %d name: ", i + 1);
        scanf("%31s", items[i].name);
        printf("Price: ");
        scanf("%d", &items[i].price);
        if (items[i].price < 0) {
            printf("Invalid price, skipping.\n");
            continue;
        }
        total += items[i].price;
    }

    do {
        printf("1) total  2) factorial  3) quit\n");
        scanf("%d", &choice);

        switch (choice) {
            case 1:
                printf("Total: %d\n", total);
                break;
            case 2:
                printf("n! = %d\n", factorial(total % 10));
                break;
            case 3:
                printf("Bye!\n");
                break;
            default:
                printf("Unknown choice\n");
        }
    } while (choice != 3);

    return EXIT_SUCCESS;
}
