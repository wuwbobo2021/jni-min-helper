package rust.jniminhelper;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;

public class InvocHdl implements InvocationHandler {
    long rust_hdl_id;

    // to be registered in native code
    private native Object rustHdl(long id, Method method, Object[] args) throws Throwable;
    
    public InvocHdl(long id) {
        this.rust_hdl_id = id;
    }

    public long getId() {
        return this.rust_hdl_id;
    }

    @Override
    public Object invoke(Object proxy, Method method, Object[] args) throws Throwable {
        String methodName = method.getName();
        if (methodName.equals("equals")) {
            if (args == null || args.length == 0) {
                return false;
            }
            if (args[0] == null || this.getClass() != args[0].getClass()) {
                return false;
            }
            InvocHdl other = (InvocHdl) args[0];
            return Boolean.valueOf(this.getId() == other.getId());
        }
        if (methodName.equals("hashCode")) {
            return Integer.valueOf(System.identityHashCode(this));
        }
        if (methodName.equals("toString")) {
            return "rust.jniminhelper.InvocHdl[" + this.rust_hdl_id + "]";
        }
        return rustHdl(this.rust_hdl_id, method, args);
    }
}
