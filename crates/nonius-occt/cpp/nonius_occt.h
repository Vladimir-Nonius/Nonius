/* Тонкий C-интерфейс к OpenCascade для Nonius.
 *
 * Только то, что нужно программе: чтение и запись STEP, контрольные числа,
 * булевы операции, сетка для отрисовки и простые тела для тестов.
 * Все строки — UTF-8. Функции не бросают исключений: ошибки OpenCascade
 * ловятся внутри и возвращаются текстом в буфер err.
 */
#ifndef NONIUS_OCCT_H
#define NONIUS_OCCT_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct NoniusShape NoniusShape;

typedef struct NoniusProps {
    double volume;
    double area;
    double center[3];
    int32_t has_bbox;
    double bbox_min[3];
    double bbox_max[3];
    uint32_t solids;
    uint32_t faces;
} NoniusProps;

typedef struct NoniusMesh {
    float* positions;   /* 3 * vertex_count */
    size_t vertex_count;
    uint32_t* indices;  /* 3 * triangle_count */
    size_t triangle_count;
} NoniusMesh;

/* Версия OpenCascade, с которой собрана прослойка: "7.9.3". */
const char* nonius_occt_version(void);

NoniusShape* nonius_read_step(const char* path, char* err, size_t err_len);
int32_t nonius_write_step(const NoniusShape* shape, const char* path, char* err, size_t err_len);
void nonius_shape_free(NoniusShape* shape);

/* Твёрдое тело номер index (с 0) внутри формы; NULL — нет такого. */
NoniusShape* nonius_shape_solid(const NoniusShape* shape, uint32_t index);

int32_t nonius_shape_props(const NoniusShape* shape, NoniusProps* out);

/* op: 0 — a минус b, 1 — общее a и b, 2 — объединение. fuzzy — допуск, мм. */
NoniusShape* nonius_boolean(const NoniusShape* a, const NoniusShape* b, int32_t op, double fuzzy,
                            char* err, size_t err_len);

/* Разность версий за одно пересечение: added = after − before, removed = before − after.
 * 1 — успех (оба результата нужно освободить), 0 — ошибка в err. */
int32_t nonius_diff(const NoniusShape* before, const NoniusShape* after, NoniusShape** added,
                    NoniusShape** removed, char* err, size_t err_len);

int32_t nonius_shape_mesh(const NoniusShape* shape, double linear_deflection,
                          double angular_deflection, NoniusMesh* out);
void nonius_mesh_free(NoniusMesh* mesh);

/* Простые тела для тестов. */
NoniusShape* nonius_make_box(double x, double y, double z, double dx, double dy, double dz);
NoniusShape* nonius_make_cylinder(double x, double y, double z, double radius, double height);

#ifdef __cplusplus
}
#endif

#endif
