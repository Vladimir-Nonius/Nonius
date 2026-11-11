// Сопоставление двух версий детали: какие рёбра, вершины и грани «стало»
// те же, что в «было». Внутренний заголовок прослойки.
//
// На этом стоят и общие грани для булевой операции (share.h), и список
// изменённых граней (faces.cpp): всё, что не сопоставилось, — изменение.

#ifndef NONIUS_MATCH_H
#define NONIUS_MATCH_H

#include <vector>

#include <TopTools_IndexedMapOfShape.hxx>
#include <TopoDS_Shape.hxx>

namespace nonius {

struct Match {
    // Рёбра, вершины и грани «было» (A) и «стало» (B); номера с 1.
    TopTools_IndexedMapOfShape EA, EB, VA, VB, FA, FB;
    // Ребро «стало» → то же ребро «было»; 0 — ребро своё.
    std::vector<int> edge;
    // Вершина «стало» → та же вершина «было»; 0 — своя.
    std::vector<int> vertex;
    // Грань «стало» → та же грань «было»; 0 — грань изменена или новая.
    std::vector<int> face;
    // Грань «было» нашлась в «стало».
    std::vector<bool> kept;
    // Сколько рёбер совпало.
    int shared_edges = 0;
};

// Сопоставить версии. false — вершины совпавших рёбер не сходятся: тела
// устроены по-разному, сопоставлению верить нельзя.
bool match_shapes(const TopoDS_Shape& before, const TopoDS_Shape& after, Match* out);

}  // namespace nonius

#endif
