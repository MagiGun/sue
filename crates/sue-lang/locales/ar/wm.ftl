## The shell (crates/sue-wm).

hub = المركز
studios = الاستوديوهات
tools = الأدوات
new = جديد
new-file-kind = جديد { $category }: { $label }
menu-file = ملف
menu-project = مشروع
menu-send-to = إرسال إلى
open-file = فتح ملف
open-file-ellipsis = فتح ملف...
open-project = فتح مشروع
open-project-ellipsis = فتح مشروع...
save-project = حفظ المشروع
save-project-as = حفظ المشروع باسم
save-project-as-ellipsis = حفظ المشروع باسم...
open-at-launch = فتح { $name } عند بدء التشغيل
command-palette = لوحة الأوامر ({ $command_palette })
check-for-updates = التحقق من وجود تحديثات
settings = الإعدادات
quit-sue = إنهاء Sue
language-menu = اللغة (Language)
language-system = النظام ({ $language })
language-follow-sue = اتبع Sue ({ $language })
language-changed = يتحدث Sue الآن { $language }
could-not-change-language = تعذر تغيير اللغة: { $error }
layout-menu = التخطيط
layout-automatic = تلقائي ({ $layout })
layout-desktop = سطح المكتب
layout-tablet = جهاز لوحي
layout-phone = هاتف
layout-phone-shaped = شكل هاتف
layout-changed = يستخدم Sue تخطيط { $layout }
could-not-change-layout = تعذر تغيير التخطيط: { $error }
sue-hub-every-studio-tool-files = Sue: المركز، وكل استوديو وأداة، والملفات، والمشاريع، والإعدادات
pinned-sue-opens-here-launch-click = مثبت: يفتح Sue هنا عند بدء التشغيل. انقر لإزالة التثبيت
pin-sue-opens-here-launch = تثبيت: يفتح Sue هنا عند بدء التشغيل
sue-opens-hub-launch = يفتح Sue على المركز عند بدء التشغيل
sue-opens-in-at-launch = يفتح Sue في { $name } عند بدء التشغيل
could-not-pin-it = تعذر التثبيت: { $error }
type-command-mode-entity-action = اكتب أمرا أو وضعا أو إجراء
nothing-matches = لا توجد تطابقات
go-to-the-hub = الانتقال إلى المركز
open = فتح { $name }
pin-open-it-at-launch = تثبيت { $name }: افتحه عند البدء
unpin-open-hub-launch = إلغاء التثبيت: افتح المركز عند البدء
open-settings = فتح الإعدادات
label-work = العمل
transfer-copy = نسخ
transfer-export = تصدير
transfer-import = استيراد
recent = الأخيرة
files = الملفات
open-recent = فتح الأخيرة
saved-projects = المشاريع المحفوظة
untitled = بلا عنوان
work-sue-keeps-you-before-saved = العمل الذي يحتفظ به Sue قبل الحفظ يظهر هنا لاستعادته أو تجاهله.
unsaved-work = العمل غير المحفوظ
files-projects-you-open-save-show = الملفات والمشاريع التي تفتحها أو تحفظها تظهر هنا.
projects-you-save-sue-menu-project = المشاريع التي تحفظها (قائمة Sue، مشروع، حفظ المشروع) تظهر هنا.
ago-just-now = للتو
ago-minutes = { $count ->
    [zero] للتو
    [one] منذ دقيقة
    [two] منذ دقيقتين
    [few] منذ { $count } دقائق
    [many] منذ { $count } دقيقة
   *[other] منذ { $count } دقيقة
}
ago-hours = { $count ->
    [zero] للتو
    [one] منذ ساعة
    [two] منذ ساعتين
    [few] منذ { $count } ساعات
    [many] منذ { $count } ساعة
   *[other] منذ { $count } ساعة
}
ago-yesterday = أمس
ago-days = { $count ->
    [zero] اليوم
    [one] منذ يوم
    [two] منذ يومين
    [few] منذ { $count } أيام
    [many] منذ { $count } يوما
   *[other] منذ { $count } يوما
}
in-folder = { $lead } في { $folder }
click-to-show-it = {"."} انقر لإظهاره
opened = تم الفتح
saved = تم الحفظ { $name }{ $note }{ $folder }{ $click }
saved-2 = تم الحفظ
saved-in-folder = {" "}في { $folder }
recover = استعادة
discard = تجاهل
really-discard = تأكيد التجاهل
kept-newer-than = تم الاحتفاظ بـ { $value }، أحدث من { $value2 }
kept-never-saved = تم الاحتفاظ بـ { $value }، لم يتم حفظه من قبل
that-is-gone = تم حذفه
entities = { $count ->
    [zero] لا توجد كائنات
    [one] كائن واحد
    [two] كائنان
    [few] { $count } كائنات
    [many] { $count } كائنا
   *[other] { $count } كائن
}
curve-pen-pressure = ضغط القلم
curve-output = المخرج
pick-a-color = اختر لونا
undo = تراجع
nothing-to-undo = لا يوجد شيء للتراجع عنه
redo = إعادة
nothing-to-redo = لا يوجد شيء لإعادته
undo-depth = تراجع { $depth }
undo-failed = فشل التراجع: { $error }
idle = خامل
fps = { $fps } إطار/ث
close-esc = إغلاق (Esc)
none = لا يوجد
there-no-studio-tool-called = لا يوجد استوديو أو أداة باسم { $id }
is-not-available = { $command } غير متاح
