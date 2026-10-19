// Functions held otherwise than by a declaration, and a test's calls.

export function makeTaskHandlers(store: Store) {
  const local = () => 0;
  return {
    async list() {
      return [];
    },
    create: async () => 1,
    updateStatus: function () {
      return 2;
    },
  };
}

export const makeRoutes = () => ({
  index() {
    return 1;
  },
  show: (id: string) => id,
});

export function mapped(items: number[]) {
  return items.map((n) => ({ hidden: () => n, also() {} }));
}

export class Store {
  load(id: string) {
    return id;
  }
  save = (x: number): number => x;
  #check = function () {};
}

export const handlers = {
  list() {
    return 1;
  },
  create: () => 2,
};

export const typed = { make: () => 3 } as const;
export const checked = { make: () => 4 } satisfies Record<string, () => number>;

register({ passed: () => 5 });

export const wrapped = withAuth(async (req: Request) => {
  return 3;
});

export const canView = (p: Project): boolean => true;

describe("Project", () => {
  it("can edit when owner", () => {
    expect(canView(p)).toBe(true);
  });
  test.skip(`cannot edit`, () => {});
  it.only("a.b c", () => {});
  test.each([1, 2])("each %i", (n) => {});
  it(`with ${1} inside`, () => {});
  describe.only("nested", () => {
    test("deep", () => {});
  });
});

suite("Other", () => {
  test.todo("later");
});

function helper() {
  it("not a test of the file", () => {});
}
