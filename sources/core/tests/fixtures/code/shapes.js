// Functions held otherwise than by a declaration, and a test's calls.

export function makeTaskHandlers(store) {
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
  show: (id) => id,
});

export function mapped(items) {
  return items.map((n) => ({ hidden: () => n, also() {} }));
}

export class Store {
  load(id) {
    return id;
  }
  save = (x) => x;
  #check = function () {};
}

export const handlers = {
  list() {
    return 1;
  },
  create: () => 2,
};


register({ passed: () => 5 });

export const wrapped = withAuth(async (req) => {
  return 3;
});

export const canView = (p) => true;

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
