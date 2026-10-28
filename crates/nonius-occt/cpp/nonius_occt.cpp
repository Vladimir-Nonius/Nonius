// Тонкий C-интерфейс к OpenCascade для Nonius. См. nonius_occt.h.

#include "nonius_occt.h"
#include "internal.h"
#include "share.h"

#include <cstdlib>
#include <cstring>
#include <exception>
#include <string>
#include <utility>
#include <vector>

#include <BOPAlgo_PaveFiller.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <BinTools.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Interface_Static.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPControl_Reader.hxx>
#include <STEPControl_Writer.hxx>
#include <Standard_Failure.hxx>
#include <Standard_Version.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

namespace {

void set_error(char* err, size_t err_len, const std::string& message) {
    if (err == nullptr || err_len == 0) {
        return;
    }
    size_t n = message.size() < err_len - 1 ? message.size() : err_len - 1;
    std::memcpy(err, message.data(), n);
    err[n] = '\0';
}

std::string failure_text(const Standard_Failure& f) {
    const char* m = f.GetMessageString();
    std::string text = f.DynamicType()->Name();
    if (m != nullptr && m[0] != '\0') {
        text += ": ";
        text += m;
    }
    return text;
}

NoniusShape* wrap(const TopoDS_Shape& shape) {
    NoniusShape* s = new NoniusShape();
    s->shape = shape;
    return s;
}

}  // namespace

extern "C" {

const char* nonius_occt_version(void) { return OCC_VERSION_COMPLETE; }

NoniusShape* nonius_read_step(const char* path, char* err, size_t err_len) {
    try {
        Interface_Static::SetCVal("xstep.cascade.unit", "MM");
        STEPControl_Reader reader;
        if (reader.ReadFile(path) != IFSelect_RetDone) {
            set_error(err, err_len, "не удалось прочитать STEP");
            return nullptr;
        }
        reader.TransferRoots();
        TopoDS_Shape shape = reader.OneShape();
        if (shape.IsNull()) {
            set_error(err, err_len, "в STEP нет геометрии");
            return nullptr;
        }
        return wrap(shape);
    } catch (const Standard_Failure& f) {
        set_error(err, err_len, failure_text(f));
    } catch (const std::exception& e) {
        set_error(err, err_len, e.what());
    } catch (...) {
        set_error(err, err_len, "неизвестная ошибка OpenCascade");
    }
    return nullptr;
}

int32_t nonius_write_step(const NoniusShape* shape, const char* path, char* err, size_t err_len) {
    try {
        Interface_Static::SetCVal("write.step.unit", "MM");
        STEPControl_Writer writer;
        if (writer.Transfer(shape->shape, STEPControl_AsIs) != IFSelect_RetDone) {
            set_error(err, err_len, "не удалось перевести тело в STEP");
            return 0;
        }
        if (writer.Write(path) != IFSelect_RetDone) {
            set_error(err, err_len, "не удалось записать STEP");
            return 0;
        }
        return 1;
    } catch (const Standard_Failure& f) {
        set_error(err, err_len, failure_text(f));
    } catch (...) {
        set_error(err, err_len, "неизвестная ошибка OpenCascade");
    }
    return 0;
}

void nonius_shape_free(NoniusShape* shape) { delete shape; }

int32_t nonius_write_brep(const NoniusShape* shape, const char* path) {
    try {
        return BinTools::Write(shape->shape, path) ? 1 : 0;
    } catch (...) {
        return 0;
    }
}

NoniusShape* nonius_read_brep(const char* path) {
    try {
        TopoDS_Shape shape;
        if (!BinTools::Read(shape, path) || shape.IsNull()) {
            return nullptr;
        }
        return wrap(shape);
    } catch (...) {
        return nullptr;
    }
}

NoniusShape* nonius_boolean(const NoniusShape* a, const NoniusShape* b, int32_t op, double fuzzy,
                            char* err, size_t err_len) {
    try {
        TopTools_ListOfShape args;
        args.Append(a->shape);
        TopTools_ListOfShape tools;
        tools.Append(b->shape);

        BRepAlgoAPI_BooleanOperation* algo = nullptr;
        BRepAlgoAPI_Cut cut;
        BRepAlgoAPI_Common common;
        BRepAlgoAPI_Fuse fuse;
        switch (op) {
            case 0: algo = &cut; break;
            case 1: algo = &common; break;
            case 2: algo = &fuse; break;
            default:
                set_error(err, err_len, "неизвестная булева операция");
                return nullptr;
        }
        algo->SetArguments(args);
        algo->SetTools(tools);
        algo->SetNonDestructive(Standard_True);
        algo->SetRunParallel(Standard_True);
        if (fuzzy > 0) {
            algo->SetFuzzyValue(fuzzy);
        }
        algo->Build();
        if (!algo->IsDone() || algo->HasErrors()) {
            set_error(err, err_len, "булева операция не удалась");
            return nullptr;
        }
        return wrap(algo->Shape());
    } catch (const Standard_Failure& f) {
        set_error(err, err_len, failure_text(f));
    } catch (...) {
        set_error(err, err_len, "неизвестная ошибка OpenCascade");
    }
    return nullptr;
}

int32_t nonius_diff(const NoniusShape* before, const NoniusShape* after, NoniusShape** added,
                    NoniusShape** removed, char* err, size_t err_len) {
    *added = nullptr;
    *removed = nullptr;
    try {
        // Пересечение тел — самая дорогая часть — считается один раз,
        // из него получаются обе разности. Без допуска: с ним обратная
        // разность из общего пересечения бывает неверной (проверено на OCCT 7.9).
        // Одинаковые грани версий делаются общими (share.h) — пересекается
        // только изменённое.
        TopoDS_Shape was, now;
        nonius::share_faces(before->shape, after->shape, &was, &now);
        TopTools_ListOfShape args;
        args.Append(was);
        args.Append(now);
        BOPAlgo_PaveFiller filler;
        filler.SetArguments(args);
        filler.SetRunParallel(Standard_True);
        filler.SetNonDestructive(Standard_True);
        filler.Perform();
        if (filler.HasErrors()) {
            set_error(err, err_len, "не удалось пересечь тела");
            return 0;
        }
        BRepAlgoAPI_Cut gone(was, now, filler, Standard_True);
        BRepAlgoAPI_Cut appeared(was, now, filler, Standard_False);
        if (!gone.IsDone() || gone.HasErrors() || !appeared.IsDone() || appeared.HasErrors()) {
            set_error(err, err_len, "булева операция не удалась");
            return 0;
        }
        *removed = wrap(gone.Shape());
        *added = wrap(appeared.Shape());
        return 1;
    } catch (const Standard_Failure& f) {
        set_error(err, err_len, failure_text(f));
    } catch (...) {
        set_error(err, err_len, "неизвестная ошибка OpenCascade");
    }
    delete *added;
    delete *removed;
    *added = nullptr;
    *removed = nullptr;
    return 0;
}

int32_t nonius_shape_mesh(const NoniusShape* shape, double linear_deflection,
                          double angular_deflection, NoniusMesh* out) {
    std::memset(out, 0, sizeof(NoniusMesh));
    try {
        TopoDS_Shape s = shape->shape;
        BRepMesh_IncrementalMesh mesher(s, linear_deflection, Standard_False, angular_deflection,
                                        Standard_True);
        (void)mesher;

        std::vector<float> positions;
        std::vector<uint32_t> indices;
        for (TopExp_Explorer ex(s, TopAbs_FACE); ex.More(); ex.Next()) {
            const TopoDS_Face& face = TopoDS::Face(ex.Current());
            TopLoc_Location loc;
            Handle(Poly_Triangulation) tri = BRep_Tool::Triangulation(face, loc);
            if (tri.IsNull()) {
                continue;
            }
            const gp_Trsf trsf = loc.Transformation();
            const uint32_t base = static_cast<uint32_t>(positions.size() / 3);
            for (Standard_Integer i = 1; i <= tri->NbNodes(); ++i) {
                gp_Pnt p = tri->Node(i).Transformed(trsf);
                positions.push_back(static_cast<float>(p.X()));
                positions.push_back(static_cast<float>(p.Y()));
                positions.push_back(static_cast<float>(p.Z()));
            }
            const bool reversed = face.Orientation() == TopAbs_REVERSED;
            for (Standard_Integer i = 1; i <= tri->NbTriangles(); ++i) {
                Standard_Integer n1, n2, n3;
                tri->Triangle(i).Get(n1, n2, n3);
                if (reversed) {
                    std::swap(n2, n3);
                }
                indices.push_back(base + static_cast<uint32_t>(n1 - 1));
                indices.push_back(base + static_cast<uint32_t>(n2 - 1));
                indices.push_back(base + static_cast<uint32_t>(n3 - 1));
            }
        }

        out->vertex_count = positions.size() / 3;
        out->triangle_count = indices.size() / 3;
        if (!positions.empty()) {
            out->positions = static_cast<float*>(std::malloc(positions.size() * sizeof(float)));
            std::memcpy(out->positions, positions.data(), positions.size() * sizeof(float));
        }
        if (!indices.empty()) {
            out->indices = static_cast<uint32_t*>(std::malloc(indices.size() * sizeof(uint32_t)));
            std::memcpy(out->indices, indices.data(), indices.size() * sizeof(uint32_t));
        }
        return 1;
    } catch (...) {
        return 0;
    }
}

void nonius_mesh_free(NoniusMesh* mesh) {
    std::free(mesh->positions);
    std::free(mesh->indices);
    std::memset(mesh, 0, sizeof(NoniusMesh));
}

NoniusShape* nonius_make_box(double x, double y, double z, double dx, double dy, double dz) {
    try {
        return wrap(BRepPrimAPI_MakeBox(gp_Pnt(x, y, z), dx, dy, dz).Shape());
    } catch (...) {
        return nullptr;
    }
}

NoniusShape* nonius_make_cylinder(double x, double y, double z, double radius, double height) {
    try {
        gp_Ax2 axis(gp_Pnt(x, y, z), gp::DZ());
        return wrap(BRepPrimAPI_MakeCylinder(axis, radius, height).Shape());
    } catch (...) {
        return nullptr;
    }
}

}  // extern "C"
