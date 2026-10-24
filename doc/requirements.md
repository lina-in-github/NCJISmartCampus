一个web智慧校园小程序
注：以下所有内容如无注明，以班级为单位(学生以自身为单位)。一个班级有多名学生与多名老师。
每一个auth只能对应一个班级，如要切换班级必须切换auth
一个user（/login中）可对应多个auth
路由表：
/ redirect /login or /home
/login 选择家长（/login/<mob/pc>/parent）、老师（/login/<mob/pc>/teacher）、管理员（/login/<mob/pc>/admin）登录
/home：redirect /home/mob/parent /home/mob/teacher /home/mob/admin /home/pc/parent /home/pc/teacher /home/pc/admin
/api/upload 
{
    auth:str //author api key
    which:str //id of submission's call
    title:str //the title of the submission,like "my maths homework" or "leave request"
    desc:str // desctription
    data:Array<Any>//datas with base64
}
返回：

{
    id:str //upload's id
}
/api/download //permission:teacher/admin
参数：
{
    auth:str
    which:str
    match_mode:enum MatchMode{
        All,
        Random(quantity:int),
        StudentID(id:str),
        ShortStudentID(id:int),
        TitleReGex(re:Regex),
        Before(time:During)
    }
}
返回值：
{
    datas:Array<JSON{
        studentID:str
        shortStudentID:int
        title:str
        desc:str
        data:Array<Any>
        reply:Array<str>
    }>
}
/api/new-submission //permission:teacher/admin
{
    auth:str
    name:str //submission's name
    desc:str //its description
    type:str //example "homework" "open call" "leave request"
    deadLine:TimeClick
}
返回：
{
    id:str
}
/api/all-submission //单位：学生为自身，教师/管理为班级
{
    auth:str
    match_mode:enum{
        
    }
}
返回：
{
    datas:Arrar<JSON{
        name:str
        desc:str
        deadLine:TimeClick
    }>
}
/api/reply //permission:teacher/admin
{
    auth:str
    uploadID:str
    content:str
}
返回：success/fail
/api/new-class //permission:admin
{
    auth:str
    desc:str
}
返回：
{
    id:str //class id
}
/api/new-user //perm:admin
{
    auth:str
    classID:str
    quantity:int
    type:enum{
        Student
        Teacher
        Admin
    }
}
return:
{
    IDs:Array<str>
}
/api/del-key
{
    auth:str
}
return:success/fail
/api/del-object //perm:admin
{
    id:str //object id
}
return success/fail
/home/*/student：
要求：通过/api与后端交互
./submissions 所有征集
./uploads 所有上传
./submission/<submission id>/upload:上传表单到征集页面

./submission/<submission id>/all:该征集的所有上传

./submission/<submission id>/:有上传跳最后一个上传，否则跳上传表单
./upload/<id>/:上传主页面（可看所有reply或重新上传）
./:通过卡片显示的主页，显示最近的征集与有reply的上传与user主页按钮
/home/*/teachers:
./submissions 所有征集(有新增按钮)
./submissions/new 新增征集表单
./uploads 所有上传
./submission/<submission id>/upload:上传表单到征集页面

./submission/<submission id>/all:该征集的所有上传

./submission/<submission id>/:有上传跳最后一个上传，否则跳上传表单
./upload/<id>/:上传主页面（可看所有reply或写reply）
./:卡片形式的主页，显示最近的征集与未reply上传与user主页按钮

/home/*/admin:
./submissions 所有征集(有新增按钮/删除按钮)
./submissions/new 新增征集表单
./uploads 所有上传
./submission/<submission id>/upload:上传表单到征集页面

./submission/<submission id>/all:该征集的所有上传

./submission/<submission id>/:有上传跳最后一个上传，否则跳上传表单
./upload/<id>/:上传主页面（可看所有reply或写reply或删除该reply）
./:主页显示最近的征集与最近的上传以及user主页按钮
/user:
user主页，可更改偏好设置、新增班级（只有当在某个班级为admin时）、增加/删除auth等（v0.1.1:为某个auth改名）
需要用户名/密码登录
---
v0.1.1新增：
/api/rename-auth
{
    auth:str
    name:str
}
返回：success/fail
---
v0.1.2:
把所有UI字符串移到zh-CN.txt;api中使用英语。
zh-CN.txt示例：
name=title lines=0 content=智慧校园
name=admin_slogan lines=2 content=
让班级的每件事，
都有迹可循
...
在服务器启动时读取strings项目，如有变化同步到strings.db。
自动替换形如@[strings/title]的标签。
strings应当可作为{}格式化的参数