from fixmod.calc import compute


class PlannedCoordinator:
    def snapshot(self, points: list) -> list:
        reference = compute(points)
        return reference
