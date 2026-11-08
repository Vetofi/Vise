import unittest
from tools import test_feature_sort_parity as sort


class FeatureSortCorpusTests(unittest.TestCase):
    def test_identity_duplicates_step_gaps_and_resource_orderings(self):
        resources = {'a': [[], ['vise:a', 'vise:b']], 'b': [['vise:c'], ['vise:a']]}
        fixtures = list(sort.corpus(resources))
        self.assertEqual(fixtures, list(sort.corpus(resources)))
        self.assertTrue(any(f['biomes'] == [[[0, 1]], [[1, 0]]] for f in fixtures))
        self.assertTrue(any(f['biomes'] == [[[0, 0]], [[]]] for f in fixtures))
        self.assertTrue(any(f['biomes'] == [] for f in fixtures))
        self.assertTrue(any(f['biomes'] == [[], [[], [], []]] for f in fixtures))
        self.assertTrue(any(len(f['biomes']) == 2 and f['biomes'][0] == [[], [0, 1]] and f['biomes'][1] == [[2], [0]] for f in fixtures))


if __name__ == '__main__':
    unittest.main()
