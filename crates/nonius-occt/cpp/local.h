// Разница версий только там, где они отличаются. Внутренний заголовок прослойки.
//
// Обычная булева операция пересекает тела целиком, хотя изменилась малая
// часть. Здесь берутся только грани, которые есть в одной из версий: вместе
// они замыкают все изменённые области. Грани делят пространство на ячейки,
// и про каждую ячейку остаётся узнать, в какой версии она заполнена
// материалом. Совпавшие грани в расчёте не участвуют вовсе.

#ifndef NONIUS_LOCAL_H
#define NONIUS_LOCAL_H

#include <TopoDS_Shape.hxx>

namespace nonius {

// `was` и `now` — версии с общими гранями (share.h), `before` и `after` —
// исходные тела. added — ячейки, которые есть только в «стало», removed —
// только в «было». false — посчитать не удалось: нужна обычная операция.
bool local_diff(const TopoDS_Shape& was, const TopoDS_Shape& now, const TopoDS_Shape& before,
                const TopoDS_Shape& after, TopoDS_Shape* added, TopoDS_Shape* removed);

}  // namespace nonius

#endif
